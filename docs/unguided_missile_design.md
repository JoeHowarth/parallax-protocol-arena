# Unguided Missile Weapon Design

## Overview
The UnguidedMissile will be a new weapon type similar to PlasmaCannon but with constant thrust propulsion instead of an initial impulse. This creates a different tactical experience where missiles have a more predictable trajectory but can potentially be outmaneuvered.

## Core Components

### UnguidedMissilePlugin
- Registers the UnguidedMissile component
- Adds FireUnguidedMissile event
- Registers systems for:
  - Missile firing
  - Missile propulsion
  - Debug input handling

### Components

#### UnguidedMissile Component
```rust
#[derive(Component, Reflect, Debug, Default)]
pub struct UnguidedMissile {
    /// Tick when this launcher will be able to fire again
    pub ready_tick: u64,
}
```

#### MissileProjectile Component
```rust
#[derive(Component, Reflect, Debug)]
struct MissileProjectile {
    /// Constant thrust force applied each tick
    thrust: f32,
    /// How long the missile will live (in ticks)
    lifetime: u64,
    /// Tick when missile was spawned
    spawn_tick: u64,
}
```

## Systems

### Firing System
- Handles FireUnguidedMissile events
- Checks cooldown (ready_tick)
- Spawns MissileProjectile entities
- Updates launcher cooldown

### Propulsion System
- Applies constant thrust force to missiles each tick
- Handles missile lifetime/despawning
- Could add slight randomization to thrust direction for realism

### Debug Input System
- Similar to PlasmaCannon's debug system
- Binds to a different key (e.g., 'M' for missile)

## Physics Configuration

### Initial State
- Spawns slightly in front of the firing craft
- Inherits firing craft's velocity
- Initial speed boost smaller than PlasmaCannon
- Aligned with firing craft's rotation

### Continuous Forces
- Constant thrust force (e.g., 50 units/tick)
- No rotation/turning capability
- Potentially affected by gravity if implemented

### Collision Properties
- Lower mass than PlasmaCannon (more realistic physics)
- Smaller hitbox
- Destroys both missile and target on collision

## Visual Design
- Elongated sprite shape
- Particle effects for thrust
- Orange/red color scheme
- Scale: smaller than PlasmaCannon projectile

## Implementation Plan

1. Create new file `src/subsystems/unguided_missile.rs`
2. Implement basic component and plugin structure
3. Add firing system with cooldown
4. Implement propulsion system
5. Add debug controls
6. Fine-tune physics parameters
7. Add visual effects
8. Test and balance gameplay

## Testing Considerations

1. Verify missile behavior:
   - Constant acceleration
   - Proper lifetime/despawn
   - Collision handling
2. Test cooldown mechanics
3. Verify proper inheritance of shooter's physics state
4. Performance testing with multiple missiles

## Future Enhancements

1. Smoke trail effects
2. Variable thrust patterns
3. Proximity detonation
4. Multiple missile types (heavy/light)
5. Sound effects