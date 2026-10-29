//! Read-only measurements for research tools (speed sweeps, drive tuning): what each
//! wheel ray saw and pushed last step, and every point where the body or roof touches
//! something. Nothing here changes the simulation; the crash rules keep using
//! `contacts::read_body_contacts`.

use rapier3d::prelude::*;
use serde::Serialize;

use crate::car::Car;
use crate::conventions::WHEEL_COUNT;

/// A contact point counts as touching when the shapes are this close or overlapping
/// (the same rule as `contacts`).
const TOUCH_DISTANCE_M: f32 = 0.005;

/// What one raycast wheel saw and did on the last physics step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct WheelDiagnostics {
    /// The ray hit something.
    pub in_contact: bool,
    /// The ground's normal where the ray hit, world frame (unit vector; straight up when
    /// not touching).
    pub ground_normal: [f32; 3],
    /// How far the spring is pushed in, in m (0 at rest length, the max travel when
    /// bottomed out; negative when extended past rest).
    pub compression_m: f32,
    /// The suspension force Rapier computed, before its per-wheel cap, in N.
    pub suspension_force_n: f32,
    /// Forward (rolling-direction) impulse the tyre applied, in N·s.
    pub forward_impulse_n_s: f32,
    /// Sideways impulse the tyre applied, in N·s.
    pub side_impulse_n_s: f32,
    /// The engine force asked of this wheel, in N.
    pub engine_force_n: f32,
}

/// Which chassis collider a contact is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChassisPart {
    /// The main body box.
    Body,
    /// The thin roof box.
    Roof,
}

/// One touching point between the body or roof and something else, world frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ChassisContactPoint {
    pub part: ChassisPart,
    /// The other collider's `user_data` (`course_build::OBSTACLE_USER_DATA` for car rows,
    /// bumps and walls, 0 for the floor and ramps).
    pub other_user_data: u128,
    /// The other collider's shape kind: "cuboid" (floor, walls, car rows), "trimesh"
    /// (ramps, kickers, corkscrews, halfpipes) or "convex" (bumps).
    pub other_shape: &'static str,
    /// Where the point is, in m.
    pub point_m: [f32; 3],
    /// Unit normal pointing from the car into the other collider.
    pub normal: [f32; 3],
}

/// The diagnostics of the car's four wheels, in wheel order FL, FR, RL, RR.
pub(crate) fn wheel_diagnostics(car: &Car) -> [WheelDiagnostics; WHEEL_COUNT] {
    let mut out = [WheelDiagnostics::default(); WHEEL_COUNT];
    for (index, wheel) in car.wheels().iter().enumerate() {
        let ray = wheel.raycast_info();
        out[index] = WheelDiagnostics {
            in_contact: ray.is_in_contact,
            ground_normal: ray.contact_normal_ws.to_array(),
            compression_m: wheel.suspension_rest_length - ray.suspension_length,
            suspension_force_n: wheel.wheel_suspension_force,
            forward_impulse_n_s: wheel.forward_impulse,
            side_impulse_n_s: wheel.side_impulse,
            engine_force_n: wheel.engine_force,
        };
        if !ray.is_in_contact {
            out[index].ground_normal = [0.0, 1.0, 0.0];
        }
    }
    out
}

/// Every touching point of the body and roof colliders after the last step.
pub(crate) fn chassis_contact_points(
    physics: &PhysicsWorld,
    car: &Car,
) -> Vec<ChassisContactPoint> {
    let mut points = Vec::new();
    for (collider, part) in [
        (car.body_collider, ChassisPart::Body),
        (car.roof_collider, ChassisPart::Roof),
    ] {
        for pair in physics.narrow_phase.contact_pairs_with(collider) {
            push_touching_points(physics, pair, collider, part, &mut points);
        }
    }
    points
}

/// Adds the touching points of one contact pair, with the normal flipped to point from
/// the car into the other collider (Rapier's points from collider1 to collider2).
fn push_touching_points(
    physics: &PhysicsWorld,
    pair: &ContactPair,
    our_collider: ColliderHandle,
    part: ChassisPart,
    points: &mut Vec<ChassisContactPoint>,
) {
    let we_are_first = pair.collider1 == our_collider;
    let other = if we_are_first {
        pair.collider2
    } else {
        pair.collider1
    };
    let other_collider = &physics.colliders[other];
    let first_collider_pose = physics.colliders[pair.collider1].position();
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
            points.push(ChassisContactPoint {
                part,
                other_user_data: other_collider.user_data,
                other_shape: shape_kind(other_collider.shape()),
                point_m: (first_collider_pose * point.local_p1).to_array(),
                normal: normal.to_array(),
            });
        }
    }
}

/// A short name for a collider's shape kind.
fn shape_kind(shape: &dyn Shape) -> &'static str {
    match shape.shape_type() {
        ShapeType::Cuboid => "cuboid",
        ShapeType::TriMesh => "trimesh",
        ShapeType::ConvexPolyhedron => "convex",
        _ => "other",
    }
}
