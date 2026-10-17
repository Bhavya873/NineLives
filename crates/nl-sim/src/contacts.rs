//! Reads what the car's body and roof touched during a physics step (SPEC §7.9).
//!
//! The crash rules need two facts: did the roof touch anything, and how fast did the
//! body or roof hit something along the contact normal. The impact speed must use the
//! velocity from *before* the step, because the solver has already stopped the car
//! by the time the step ends.

use rapier3d::prelude::*;

/// The chassis motion saved just before a physics step.
#[derive(Debug, Clone, Copy)]
pub struct MotionBeforeStep {
    /// Velocity of the centre of mass, world frame.
    pub linear_velocity_m_s: Vector,
    /// Angular velocity, world frame.
    pub angular_velocity_rad_s: Vector,
    /// Centre of mass position, world frame.
    pub centre_of_mass_m: Vector,
}

impl MotionBeforeStep {
    /// Saves the motion of `body` now.
    pub fn of(body: &RigidBody) -> MotionBeforeStep {
        MotionBeforeStep {
            linear_velocity_m_s: body.linvel(),
            angular_velocity_rad_s: body.angvel(),
            centre_of_mass_m: body.center_of_mass(),
        }
    }

    /// Velocity of a point on the body at `point_m` (world coordinates).
    fn velocity_at(&self, point_m: Vector) -> Vector {
        let arm_m = point_m - self.centre_of_mass_m;
        self.linear_velocity_m_s + self.angular_velocity_rad_s.cross(arm_m)
    }
}

/// What the body and roof touched this step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyContacts {
    /// Fastest approach speed into anything, along the contact normal.
    pub impact_speed_m_s: f32,
    /// True if the roof collider touches anything.
    pub roof_touching: bool,
}

/// A contact point counts as touching when the shapes are this close or overlapping.
/// Rapier also keeps "predicted" points a few centimetres apart; those are ignored.
const TOUCH_DISTANCE_M: f32 = 0.005;

/// Looks at every touching contact point of the body and roof colliders after a step.
/// Other bodies are treated as still (true for the ground and course pieces).
pub fn read_body_contacts(
    physics: &PhysicsWorld,
    body_collider: ColliderHandle,
    roof_collider: ColliderHandle,
    before: &MotionBeforeStep,
) -> BodyContacts {
    let mut contacts = BodyContacts::default();
    for (collider, is_roof) in [(body_collider, false), (roof_collider, true)] {
        for pair in physics.narrow_phase.contact_pairs_with(collider) {
            let Some(speed_m_s) = fastest_touching_approach_m_s(physics, pair, collider, before)
            else {
                continue;
            };
            if is_roof {
                contacts.roof_touching = true;
            }
            contacts.impact_speed_m_s = contacts.impact_speed_m_s.max(speed_m_s);
        }
    }
    contacts
}

/// The fastest speed at which `our_collider` was moving into the other collider, over
/// the pair's touching points; `None` if no point is touching.
fn fastest_touching_approach_m_s(
    physics: &PhysicsWorld,
    pair: &ContactPair,
    our_collider: ColliderHandle,
    before: &MotionBeforeStep,
) -> Option<f32> {
    // Rapier's normal points from collider1 to collider2. Flip it if we are collider2,
    // so it always points from our car into the other object.
    let we_are_first = pair.collider1 == our_collider;
    let first_collider_pose = physics.colliders[pair.collider1].position();
    let mut fastest_m_s: Option<f32> = None;
    for manifold in pair.manifolds() {
        let normal = if we_are_first {
            manifold.data.normal
        } else {
            -manifold.data.normal
        };
        for point in &manifold.points {
            if point.dist > TOUCH_DISTANCE_M {
                continue;
            }
            let point_m = first_collider_pose * point.local_p1;
            let approach_m_s = before.velocity_at(point_m).dot(normal).max(0.0);
            fastest_m_s = Some(fastest_m_s.unwrap_or(0.0).max(approach_m_s));
        }
    }
    fastest_m_s
}
