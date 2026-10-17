//! Where the car comes back after a crash or a reset (SPEC §7.9).
//!
//! A checkpoint is only the car's pose and velocities. Respawning places the car there
//! with a full tank; everything else (mode, clock, random generator, stats) carries on.

use rapier3d::prelude::RigidBody;
use serde::{Deserialize, Serialize};

use crate::world::Pose;

/// The car's pose and motion at the last checkpoint, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Where the centre of mass is and which way the car faces.
    pub pose: Pose,
    /// Velocity: the car's own when set from the car, 0 for a course checkpoint line.
    pub linear_velocity_m_s: [f32; 3],
    /// Angular velocity.
    pub angular_velocity_rad_s: [f32; 3],
}

impl Checkpoint {
    /// Records where `chassis` is and how it moves right now.
    pub fn of_chassis(chassis: &RigidBody) -> Checkpoint {
        Checkpoint {
            pose: Pose::new(chassis.translation(), *chassis.rotation()),
            linear_velocity_m_s: chassis.linvel().to_array(),
            angular_velocity_rad_s: chassis.angvel().to_array(),
        }
    }
}
