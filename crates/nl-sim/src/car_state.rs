//! What a controller can know about the car (SPEC §8, §9.2).
//!
//! `CarState` is the car's physical state on one control step, shaped like the physical
//! rows of the M5 observation (`ObsSpec` will scale and pack these same values) plus the
//! tank gate the allocation needs. `CarModel` is what does not change during a run: mass,
//! inertia and what each nozzle can push.

use rapier3d::prelude::*;

use crate::car::Car;
use crate::conventions::{NOZZLE_COUNT, ON_WHEELS_MIN_WHEELS, WHEEL_COUNT};
use crate::thrusters::nozzle_force_and_torque_at_full_throttle;

/// Longest downward ray for `CarState::height_m` (SPEC §9.2: capped at 20 m).
pub const MAX_HEIGHT_M: f32 = 20.0;
/// Rows of a wrench: force x, y, z then torque x, y, z (body frame).
pub const WRENCH_SIZE: usize = 6;
/// The first `FORCE_ROWS` rows of a wrench are force; the rest are torque.
pub const FORCE_ROWS: usize = 3;

/// A force and torque on the car: [force x, y, z, torque x, y, z] in the body frame.
pub type Wrench = [f32; WRENCH_SIZE];

/// The car's state on one control step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarState {
    /// Unit vector of world-down in the body frame (+X forward, +Y up, +Z right).
    pub gravity_dir_body: [f32; 3],
    /// Angular velocity in the body frame: roll rate about +X, yaw about +Y, pitch about +Z.
    pub angular_velocity_body_rad_s: [f32; 3],
    /// Velocity of the centre of mass in the body frame.
    pub linear_velocity_body_m_s: [f32; 3],
    /// Distance straight down from the centre of mass to whatever is below it,
    /// capped at `MAX_HEIGHT_M`.
    pub height_m: f32,
    /// Which wheels touch the ground, in wheel order FL, FR, RL, RR.
    pub wheel_contact: [bool; WHEEL_COUNT],
    /// The mode's thrust relative to the Realistic baseline (SPEC §9.2 row 19).
    pub mode_thrust_scale: f32,
    /// How much of its thrust the tank still allows, 0..1 (SPEC §7.5).
    pub tank_gate: f32,
}

impl CarState {
    /// How many wheels touch the ground.
    pub fn wheels_down(&self) -> usize {
        self.wheel_contact.iter().filter(|down| **down).count()
    }

    /// True when the car is driving on its wheels (the same rule Hop and refill use).
    pub fn on_wheels(&self) -> bool {
        self.wheels_down() >= ON_WHEELS_MIN_WHEELS
    }

    /// How much the car's up axis points along world up: 1 upright, 0 on its side,
    /// -1 upside down.
    pub fn up_alignment(&self) -> f32 {
        -self.gravity_dir_body[1]
    }

    /// How fast the car falls (positive downward), in m/s.
    pub fn falling_speed_m_s(&self) -> f32 {
        let [gx, gy, gz] = self.gravity_dir_body;
        let [vx, vy, vz] = self.linear_velocity_body_m_s;
        gx * vx + gy * vy + gz * vz
    }
}

impl Default for CarState {
    /// A car at rest on its wheels: what tests and the Off controller can assume.
    fn default() -> CarState {
        CarState {
            gravity_dir_body: [0.0, -1.0, 0.0],
            angular_velocity_body_rad_s: [0.0; 3],
            linear_velocity_body_m_s: [0.0; 3],
            height_m: 0.6,
            wheel_contact: [true; WHEEL_COUNT],
            mode_thrust_scale: 1.0,
            tank_gate: 1.0,
        }
    }
}

/// What the car is, for controllers that plan forces (PD's allocation, SPEC §8).
#[derive(Debug, Clone, PartialEq)]
pub struct CarModel {
    pub mass_kg: f32,
    /// Principal moments of inertia about the body axes X, Y, Z.
    pub principal_inertia_kg_m2: [f32; 3],
    /// Each nozzle's push at full throttle in Realistic mode: the columns of the
    /// SPEC's 6×10 matrix `B`.
    pub nozzle_wrenches: [Wrench; NOZZLE_COUNT],
}

/// SPEC §9.2: a world-down ray from the centre of mass, ignoring the car itself.
pub(crate) fn height_above_ground_m(physics: &PhysicsWorld, car: &Car) -> f32 {
    let body = &physics.bodies[car.chassis];
    let ray = Ray::new(body.translation(), Vector::new(0.0, -1.0, 0.0));
    let filter = QueryFilter::default().exclude_rigid_body(car.chassis);
    match physics.cast_ray(&ray, MAX_HEIGHT_M, true, filter) {
        Some((_, distance_m)) => distance_m,
        None => MAX_HEIGHT_M,
    }
}

/// Reads the car's mass, inertia and nozzle pushes.
pub(crate) fn read_car_model(physics: &PhysicsWorld, car: &Car) -> CarModel {
    let body = &physics.bodies[car.chassis];
    // The body and roof boxes are symmetric about the body axes, so the principal
    // axes are the body axes.
    let inertia = body.mass_properties().local_mprops.principal_inertia();
    let mut nozzle_wrenches = [[0.0; WRENCH_SIZE]; NOZZLE_COUNT];
    for (index, nozzle) in car.config().thrusters.nozzles.iter().enumerate() {
        let (force_n, torque_n_m) = nozzle_force_and_torque_at_full_throttle(nozzle);
        let [fx, fy, fz] = force_n.to_array();
        let [tx, ty, tz] = torque_n_m.to_array();
        nozzle_wrenches[index] = [fx, fy, fz, tx, ty, tz];
    }
    CarModel {
        mass_kg: body.mass(),
        principal_inertia_kg_m2: inertia.to_array(),
        nozzle_wrenches,
    }
}
