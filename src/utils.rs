use bevy::prelude::Vec2;

/// Convert screen coordinates to world coordinates
/// Screen: (0,0) at top-left, +y down
/// World: (0,0) at center, +y up
pub fn screen_to_world(screen_pos: Vec2) -> Vec2 {
    Vec2::new(screen_pos.x, -screen_pos.y)  // Just flip y for now
}

/// Convert world coordinates to screen coordinates
pub fn world_to_screen(world_pos: Vec2) -> Vec2 {
    Vec2::new(world_pos.x, -world_pos.y)  // Same operation, just documenting intent
}

/// Convert a direction vector from screen to world space
/// This only flips the y component since it's a relative vector
pub fn screen_dir_to_world(screen_dir: Vec2) -> Vec2 {
    Vec2::new(screen_dir.x, -screen_dir.y)
}

/// Calculate intersection point of a ray originating inside an AABB
///
/// # Arguments
/// * `min` - Minimum point of the AABB
/// * `max` - Maximum point of the AABB
/// * `origin` - Ray origin point (must be inside AABB)
/// * `direction` - Ray direction (need not be normalized)
///
/// # Returns
/// * `Ok(Vec2)` - Intersection point with AABB boundary
/// * `Err(IntersectError)` - If ray origin is outside or direction is zero
pub fn intersect_ray_aabb(
    min: Vec2,
    max: Vec2,
    origin: Vec2,
    direction: Vec2,
) -> Result<Vec2, IntersectError> {
    const EPSILON: f32 = 1e-6;

    // Validate origin is inside
    if origin.x < min.x
        || origin.x > max.x
        || origin.y < min.y
        || origin.y > max.y
    {
        return Err(IntersectError::OriginOutside);
    }

    // Validate direction
    if direction.x.abs() < EPSILON && direction.y.abs() < EPSILON {
        return Err(IntersectError::ZeroDirection);
    }

    // Initialize t to infinity
    let mut t = f32::INFINITY;

    // Check X axis if moving in X
    if direction.x.abs() > EPSILON {
        let tx = if direction.x > 0.0 {
            (max.x - origin.x) / direction.x
        } else {
            (min.x - origin.x) / direction.x
        };
        t = t.min(tx);
    }

    // Check Y axis if moving in Y
    if direction.y.abs() > EPSILON {
        let ty = if direction.y > 0.0 {
            (max.y - origin.y) / direction.y
        } else {
            (min.y - origin.y) / direction.y
        };
        t = t.min(ty);
    }

    Ok(origin + direction * t)
}

#[derive(Debug, PartialEq)]
pub enum IntersectError {
    OriginOutside,
    ZeroDirection,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-6;

    fn assert_vec2_eq(a: Vec2, b: Vec2) {
        assert!(
            (a - b).length_squared() < EPSILON,
            "Vector inequality: {:?} != {:?}",
            a,
            b
        );
    }

    #[test]
    fn test_basic_intersections() {
        let min = Vec2::new(-1.0, -1.0);
        let max = Vec2::new(1.0, 1.0);

        // Test cardinal directions
        let cases = [
            (Vec2::X, Vec2::new(1.0, 0.0)),
            (Vec2::Y, Vec2::new(0.0, 1.0)),
            (-Vec2::X, Vec2::new(-1.0, 0.0)),
            (-Vec2::Y, Vec2::new(0.0, -1.0)),
        ];

        for (dir, expected) in cases {
            let point = intersect_ray_aabb(min, max, Vec2::ZERO, dir).unwrap();
            assert_vec2_eq(point, expected);
        }
    }

    #[test]
    fn test_diagonal_intersections() {
        let min = Vec2::new(-1.0, -1.0);
        let max = Vec2::new(1.0, 1.0);

        let dir = Vec2::ONE.normalize();
        let point = intersect_ray_aabb(min, max, Vec2::ZERO, dir).unwrap();

        // Should hit either x=1 or y=1 exactly
        assert!(
            (point.x.abs() - 1.0).abs() < EPSILON
                || (point.y.abs() - 1.0).abs() < EPSILON
        );
    }

    #[test]
    fn test_error_cases() {
        let min = Vec2::new(-1.0, -1.0);
        let max = Vec2::new(1.0, 1.0);

        // Test origin outside box
        let result = intersect_ray_aabb(min, max, Vec2::new(2.0, 0.0), Vec2::X);
        assert_eq!(result, Err(IntersectError::OriginOutside));

        // Test zero direction
        let result = intersect_ray_aabb(min, max, Vec2::ZERO, Vec2::ZERO);
        assert_eq!(result, Err(IntersectError::ZeroDirection));
    }

    #[test]
    fn test_near_boundary_cases() {
        let min = Vec2::new(-1.0, -1.0);
        let max = Vec2::new(1.0, 1.0);

        // Test near boundary origin
        let point =
            intersect_ray_aabb(min, max, Vec2::new(0.999, 0.0), Vec2::X)
                .unwrap();
        assert_vec2_eq(point, Vec2::new(1.0, 0.0));

        // Test very small direction
        let point = intersect_ray_aabb(
            min,
            max,
            Vec2::ZERO,
            Vec2::new(EPSILON * 2.0, 0.0),
        )
        .unwrap();
        assert!((point.x - 1.0).abs() < EPSILON * 10.0);
    }
}
