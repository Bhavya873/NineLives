//! Where the car comes back after a crash or a reset (SPEC §7.9).
//!
//! A checkpoint is only the car's pose and velocities. Respawning places the car there
//! with a full tank; everything else (mode, clock, random generator, stats) carries on.

use rapier3d::prelude::RigidBody;
use serde::{Deserialize, Serialize};

/// The car's pose and motion at the last checkpoint, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Centre of mass position.
    pub position_m: [f32; 3],
    /// Orientation as a quaternion (x, y, z, w).
    pub rotation_xyzw: [f32; 4],
    /// Velocity, so a checkpoint before a jump still has the speed to clear it.
    pub linear_velocity_m_s: [f32; 3],
    /// Angular velocity.
    pub angular_velocity_rad_s: [f32; 3],
}

impl Checkpoint {
    /// Records where `chassis` is and how it moves right now.
    pub fn of_chassis(chassis: &RigidBody) -> Checkpoint {
        let rotation = chassis.rotation();
        Checkpoint {
            position_m: chassis.translation().to_array(),
            rotation_xyzw: [rotation.x, rotation.y, rotation.z, rotation.w],
            linear_velocity_m_s: chassis.linvel().to_array(),
            angular_velocity_rad_s: chassis.angvel().to_array(),
        }
    }
}
