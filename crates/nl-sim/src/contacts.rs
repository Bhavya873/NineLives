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

/// The scrape rule (docs/contracts/crash-scrape.md): which body contacts are a gentle
/// underbody touch and not an impact. All four conditions must hold. The numbers come
/// from `configs/approach.toml` (`ApproachConfig`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrapeRule {
    /// The body's half-height, in m (the lower face is this far below the body's centre).
    pub body_half_height_m: f32,
    /// The contact must be this far inside the lower face or lower, in m.
    pub underbody_margin_m: f32,
    /// The contact normal must be within this angle of body down, in rad.
    pub normal_limit_rad: f32,
    /// The normal impact speed must be at most this, in m/s.
    pub max_normal_m_s: f32,
    /// The car's tilt must be under this, in rad.
    pub max_tilt_rad: f32,
}

/// What the body and roof touched this step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyContacts {
    /// Fastest approach speed into anything, along the contact normal, over the contacts
    /// that are not a Scrape.
    pub impact_speed_m_s: f32,
    /// True if the roof collider touches anything.
    pub roof_touching: bool,
    /// The fastest normal speed of this step's Scrape contacts, in m/s, if any.
    pub scrape_speed_m_s: Option<f32>,
}

/// The car's pose and tilt now, for judging where on the body a contact is.
#[derive(Debug, Clone, Copy)]
pub struct BodyPose {
    /// The chassis position and orientation, world frame.
    pub pose: Pose3,
    /// The angle between the car's up axis and world up, in rad.
    pub tilt_rad: f32,
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
    scrape: &ScrapeRule,
    body: &BodyPose,
) -> BodyContacts {
    let mut contacts = BodyContacts::default();
    for (collider, is_roof) in [(body_collider, false), (roof_collider, true)] {
        for pair in physics.narrow_phase.contact_pairs_with(collider) {
            let points = touching_points(physics, pair, collider, before);
            if !points.is_empty() && is_roof {
                contacts.roof_touching = true;
            }
            for point in points {
                if !is_roof && is_scrape(&point, scrape, body) {
                    let fastest = contacts.scrape_speed_m_s.unwrap_or(0.0);
                    contacts.scrape_speed_m_s = Some(fastest.max(point.approach_m_s));
                } else {
                    contacts.impact_speed_m_s = contacts.impact_speed_m_s.max(point.approach_m_s);
                }
            }
        }
    }
    contacts
}

/// One touching contact point: where it is, which way its normal points (from the car
/// into the other collider) and how fast the car was moving into it.
#[derive(Debug, Clone, Copy)]
struct TouchingPoint {
    point_m: Vector,
    normal: Vector,
    approach_m_s: f32,
}

/// True if the contact `point` is a Scrape under `rule` (docs/contracts/crash-scrape.md).
fn is_scrape(point: &TouchingPoint, rule: &ScrapeRule, body: &BodyPose) -> bool {
    let inverse = body.pose.rotation.inverse();
    let point_body_m = inverse * (point.point_m - body.pose.translation);
    let normal_body = inverse * point.normal;
    is_underbody_scrape(
        point_body_m.to_array(),
        normal_body.to_array(),
        point.approach_m_s,
        body.tilt_rad,
        rule,
    )
}

/// The four scrape conditions on numbers already in the body frame (+Y up). The normal
/// points from the car into the other collider, so an underbody contact has it pointing
/// down the body (-Y).
pub fn is_underbody_scrape(
    point_body_m: [f32; 3],
    normal_body: [f32; 3],
    normal_speed_m_s: f32,
    tilt_rad: f32,
    rule: &ScrapeRule,
) -> bool {
    let underbody = point_body_m[1] < -(rule.body_half_height_m - rule.underbody_margin_m);
    let length = (normal_body[0] * normal_body[0]
        + normal_body[1] * normal_body[1]
        + normal_body[2] * normal_body[2])
        .sqrt()
        .max(1e-6);
    let normal_down_cos = -normal_body[1] / length;
    let normal_points_up = normal_down_cos >= rule.normal_limit_rad.cos();
    let gentle = normal_speed_m_s <= rule.max_normal_m_s;
    let level = tilt_rad < rule.max_tilt_rad;
    underbody && normal_points_up && gentle && level
}

/// True if `collider` has any touching contact point with anything (SPEC §10's
/// touchdown: the first moment the car meets the ground, wheels or not).
pub fn is_touching(physics: &PhysicsWorld, collider: ColliderHandle) -> bool {
    is_touching_where(physics, collider, |_| true)
}

/// True if `collider` touches a collider whose `user_data` is `user_data` (e.g.
/// `course_build::OBSTACLE_USER_DATA`).
pub fn is_touching_tagged(
    physics: &PhysicsWorld,
    collider: ColliderHandle,
    user_data: u128,
) -> bool {
    is_touching_where(physics, collider, |other| {
        physics.colliders[other].user_data == user_data
    })
}

/// True if `collider` touches (within `TOUCH_DISTANCE_M`) another collider that `wanted`
/// accepts.
fn is_touching_where(
    physics: &PhysicsWorld,
    collider: ColliderHandle,
    wanted: impl Fn(ColliderHandle) -> bool,
) -> bool {
    for pair in physics.narrow_phase.contact_pairs_with(collider) {
        let other = if pair.collider1 == collider {
            pair.collider2
        } else {
            pair.collider1
        };
        if !wanted(other) {
            continue;
        }
        for manifold in pair.manifolds() {
            for point in &manifold.points {
                if point.dist <= TOUCH_DISTANCE_M {
                    return true;
                }
            }
        }
    }
    false
}

/// A contact point on the floor counts as being on its top face above this height, in m
/// (the floor's top is at y = 0).
const FLOOR_TOP_MIN_Y_M: f32 = -0.05;

/// True for the floor: a box that is not an obstacle. Its top is flat, but when the car's
/// bevelled body edge meets it at speed, Rapier's box-box contact can report a normal
/// tipped far from vertical (R2c found 244 such crashes of 1295, with "impact speeds"
/// up to 48 m/s for a car sliding at 50 m/s with its rear edge on the ground). The speed
/// into the floor is the speed along its true normal, straight down.
fn is_flat_floor(collider: &Collider) -> bool {
    collider.shape().shape_type() == ShapeType::Cuboid && collider.user_data == 0
}

/// The touching contact points of `pair`, with the normal flipped to point from our
/// collider into the other one and the speed at which our collider was moving into it
/// (from the motion before the step). Empty if no point is touching.
fn touching_points(
    physics: &PhysicsWorld,
    pair: &ContactPair,
    our_collider: ColliderHandle,
    before: &MotionBeforeStep,
) -> Vec<TouchingPoint> {
    // Rapier's normal points from collider1 to collider2. Flip it if we are collider2,
    // so it always points from our car into the other object.
    let we_are_first = pair.collider1 == our_collider;
    let other = if we_are_first {
        pair.collider2
    } else {
        pair.collider1
    };
    let other_is_floor = is_flat_floor(&physics.colliders[other]);
    let first_collider_pose = physics.colliders[pair.collider1].position();
    let mut points = Vec::new();
    for manifold in pair.manifolds() {
        let reported_normal = if we_are_first {
            manifold.data.normal
        } else {
            -manifold.data.normal
        };
        for point in &manifold.points {
            if point.dist > TOUCH_DISTANCE_M {
                continue;
            }
            let point_m = first_collider_pose * point.local_p1;
            // The floor is flat: whatever the solver reports, the surface under a point on
            // its top pushes straight up (see `is_flat_floor`).
            let normal = if other_is_floor && point_m.y > FLOOR_TOP_MIN_Y_M {
                Vector::NEG_Y
            } else {
                reported_normal
            };
            let approach_m_s = before.velocity_at(point_m).dot(normal).max(0.0);
            points.push(TouchingPoint {
                point_m,
                normal,
                approach_m_s,
            });
        }
    }
    points
}
