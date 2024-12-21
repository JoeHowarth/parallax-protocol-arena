use assert_approx_eq::assert_approx_eq;
use bevy::utils::default;

use crate::{
    physics::{
        PhysicsBundle,
        PhysicsSimulationPlugin,
        PhysicsState,
        SimulationConfig,
    },
    prelude::*,
};

/// Standard test configuration for predictable physics simulation
pub const TEST_CONFIG: SimulationConfig = SimulationConfig {
    current_tick: 0,
    ticks_per_second: 1, // Use 1 TPS for easier reasoning about time
    time_dilation: 1.0,
    paused: false,
    prediction_ticks: 2,
};

/// Builder for creating test physics states
#[cfg(test)]
#[derive(Default)]
pub struct TestStateBuilder {
    state: PhysicsState,
}

impl TestStateBuilder {
    pub fn new() -> Self {
        Self {
            state: PhysicsState {
                pos: Vec2::ZERO,
                vel: Vec2::ZERO,
                rotation: 0.0,
                ang_vel: 0.0,
                mass: 1.0,
                current_thrust: 0.0,
                max_thrust: 100.0,
                alive: true,
            },
        }
    }

    pub fn pos(mut self, x: f32, y: f32) -> Self {
        self.state.pos = Vec2::new(x, y);
        self
    }

    pub fn vel(mut self, x: f32, y: f32) -> Self {
        self.state.vel = Vec2::new(x, y);
        self
    }

    pub fn mass(mut self, mass: f32) -> Self {
        self.state.mass = mass;
        self
    }

    pub fn thrust(mut self, current: f32, max: f32) -> Self {
        self.state.current_thrust = current;
        self.state.max_thrust = max;
        self
    }

    pub fn build(self) -> PhysicsState {
        self.state
    }
}

/// Represents a complete collision test scenario
#[cfg(test)]
pub struct CollisionScenario {
    pub a_state: PhysicsState,
    pub b_state: PhysicsState,
    pub dim: Vec2,
    pub ticks: u64,
    pub expected_a: ExpectedResult,
    pub expected_b: ExpectedResult,
}

#[cfg(test)]
#[derive(Debug, PartialEq)]
pub struct ExpectedResult {
    pub alive: bool,
    pub pos: Option<Vec2>,
    pub vel: Option<Vec2>,
}

impl CollisionScenario {
    /// Creates a basic head-on collision scenario
    pub fn head_on() -> Self {
        Self {
            a_state: TestStateBuilder::new()
                .pos(0., 0.)
                .vel(10., 0.)
                .mass(9.)
                .build(),
            b_state: TestStateBuilder::new().pos(20., 0.).mass(1.).build(),
            dim: Vec2::splat(2.),
            ticks: 3,
            expected_a: ExpectedResult {
                alive: true,
                pos: Some(Vec2::new(29., 0.)),
                vel: Some(Vec2::new(9., 0.)),
            },
            expected_b: ExpectedResult {
                alive: false,
                pos: None,
                vel: None,
            },
        }
    }

    /// Creates a glancing collision scenario
    pub fn glancing() -> Self {
        Self {
            a_state: TestStateBuilder::new()
                .pos(0., 1.)
                .vel(10., 0.)
                .mass(9.)
                .build(),
            b_state: TestStateBuilder::new().pos(20., 0.).mass(1.).build(),
            dim: Vec2::splat(2.),
            ticks: 3,
            expected_a: ExpectedResult {
                alive: true,
                pos: Some(Vec2::new(29., 1.)),
                vel: Some(Vec2::new(9., 0.)),
            },
            expected_b: ExpectedResult {
                alive: false,
                pos: None,
                vel: None,
            },
        }
    }

    /// Runs this scenario in a test app and returns the final states
    pub fn run(&self) -> (PhysicsState, PhysicsState) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PhysicsSimulationPlugin {
                config: TEST_CONFIG,
                schedule: Update,
                should_keep_alive: true,
            });

        // Spawn test entities
        let a = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(self.a_state.clone(), self.dim))
            .id();

        let b = app
            .world_mut()
            .spawn(PhysicsBundle::from_state(self.b_state.clone(), self.dim))
            .id();

        // Run simulation
        for _ in 0..self.ticks {
            app.update();
        }

        // Get final states
        let a_final = app.world().entity(a).get::<PhysicsState>().cloned();
        let b_final = app.world().entity(b).get::<PhysicsState>().cloned();

        (a_final.unwrap_or_default(), b_final.unwrap_or_default())
    }

    /// Asserts that final states match expected results
    pub fn assert_results(
        &self,
        a_final: &PhysicsState,
        b_final: &PhysicsState,
    ) {
        // Assert A's results
        assert_eq!(
            a_final.alive, self.expected_a.alive,
            "Entity A alive state mismatch"
        );
        if let Some(expected_pos) = self.expected_a.pos {
            assert_approx_eq!(a_final.pos.x, expected_pos.x);
            assert_approx_eq!(a_final.pos.y, expected_pos.y);
        }
        if let Some(expected_vel) = self.expected_a.vel {
            assert_approx_eq!(a_final.vel.x, expected_vel.x);
            assert_approx_eq!(a_final.vel.y, expected_vel.y);
        }

        // Assert B's results
        assert_eq!(
            b_final.alive, self.expected_b.alive,
            "Entity B alive state mismatch"
        );
        if let Some(expected_pos) = self.expected_b.pos {
            assert_approx_eq!(b_final.pos.x, expected_pos.x);
            assert_approx_eq!(b_final.pos.y, expected_pos.y);
        }
        if let Some(expected_vel) = self.expected_b.vel {
            assert_approx_eq!(b_final.vel.x, expected_vel.x);
            assert_approx_eq!(b_final.vel.y, expected_vel.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_head_on_collision() {
        let scenario = CollisionScenario::head_on();
        let (a_final, b_final) = scenario.run();
        scenario.assert_results(&a_final, &b_final);
    }

    #[test]
    fn test_glancing_collision() {
        let scenario = CollisionScenario::glancing();
        let (a_final, b_final) = scenario.run();
        scenario.assert_results(&a_final, &b_final);
    }
}
