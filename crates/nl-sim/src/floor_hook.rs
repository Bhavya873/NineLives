//! Keeps the solver's contact normal on the floor vertical.
//!
//! The floor is a flat box, but when the car's bevelled body edge meets it at speed,
//! Rapier's contact can report a normal tipped far from vertical (see
//! `contacts::is_flat_floor`). The solver then pushes the car along the direction of
//! travel and spins it: a 1.6 m/s rear-edge touchdown became a -8.8 rad/s yaw and a 7 m/s
//! bounce in one step (speed sweep, wall ramp 2 m, seed 7). This hook puts a tipped
//! chassis-versus-floor normal back to the floor's true one, straight up, for contacts
//! on the floor's top face. Vertical normals pass through untouched.

use rapier3d::prelude::*;

use crate::contacts::is_flat_floor;

/// A normal this close to the floor's own (cosine of the angle) is left alone.
const TRUE_NORMAL_COS: f32 = 0.995;
/// A contact point counts as on the top face only if it is within this far of the floor's
/// edge, inward, in m: a car hitting the box's side or end must keep its real normal.
const EDGE_MARGIN_M: f32 = 0.5;
/// ... and no further below the floor's top than this, in m.
const BELOW_TOP_M: f32 = 0.3;

/// The physics hook for the sim's step. Stateless.
pub(crate) struct FloorNormalHook;

impl PhysicsHooks for FloorNormalHook {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let colliders = context.colliders;
        let first_is_floor = is_flat_floor(&colliders[context.collider1]);
        let second_is_floor = is_flat_floor(&colliders[context.collider2]);
        if first_is_floor == second_is_floor {
            return;
        }
        let floor = &colliders[if first_is_floor {
            context.collider1
        } else {
            context.collider2
        }];
        let aabb = floor.compute_aabb();
        let Some(rigid) = context.rigid_mut() else {
            return;
        };
        // Rapier's normal points from collider 1 to collider 2.
        let true_normal = if first_is_floor {
            Vector::Y
        } else {
            -Vector::Y
        };
        if rigid.normal.dot(true_normal) >= TRUE_NORMAL_COS {
            return;
        }
        let on_top_face = |point: Vector| {
            point.x > aabb.mins.x + EDGE_MARGIN_M
                && point.x < aabb.maxs.x - EDGE_MARGIN_M
                && point.z > aabb.mins.z + EDGE_MARGIN_M
                && point.z < aabb.maxs.z - EDGE_MARGIN_M
                && point.y > aabb.maxs.y - BELOW_TOP_M
        };
        let car_side = |contact: &SolverContact| {
            if first_is_floor {
                contact.anchor2
            } else {
                contact.anchor1
            }
        };
        if !rigid
            .solver_contacts
            .iter()
            .all(|contact| on_top_face(car_side(contact)))
        {
            return;
        }
        *rigid.normal = true_normal;
        for contact in rigid.solver_contacts.iter_mut() {
            let car_point = car_side(contact);
            // Separation along the true normal: height of the car's point over the top.
            contact.dist = car_point.y - aabb.maxs.y;
            let floor_point = Vector::new(car_point.x, aabb.maxs.y, car_point.z);
            if first_is_floor {
                contact.anchor1 = floor_point;
            } else {
                contact.anchor2 = floor_point;
            }
        }
    }
}
