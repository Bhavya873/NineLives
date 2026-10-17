//! What a controller can know about the car (SPEC §8, §9.2).
//!
//! `CarState` is the car's physical state on one control step: the same quantities the
//! M5 observation (`ObsSpec`) will scale and pack, but in plain SI units. `CarModel` is
//! what does not change during a run: mass, inertia and what each nozzle can push.

use rapier3d::prelude::*;

use crate::car::Car;
use crate::conventions::NOZZLE_COUNT;
use crate::thrusters::nozzle_force_and_torque_at_full_throttle;

/// Longest downward ray for `CarState::height_m` (SPEC §9.2: capped at 20 m).
pub const MAX_HEIGHT_M: f32 = 20.0;

/// The car's state on one control step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarState {
    /// Unit vector of world-down in the body frame (+X forward, +Y up, +Z right).
    pub gravity_dir_body: [f32; 3],
    /// Angular velocity in the body frame: roll rate about +X, yaw about +Y, pitch about +Z.
    pub angular_velocity_body_rad_s: [f32; 3],
    /// Velocity of the centre of mass in world coordinates (+Y up).
    pub linear_velocity_world_m_s: [f32; 3],
    /// Distance straight down from the centre of mass to whatever is below it,
    /// capped at `MAX_HEIGHT_M`.
    pub height_m: f32,
    /// How many wheels touch the ground (0..4).
    pub wheels_down: usize,
    /// How much the car's up axis points along world up: 1 upright, 0 on its side,
    /// -1 upside down.
    pub up_alignment: f32,
    /// Thrust relative to the Realistic baseline (the mode's scale times the tank gate),
    /// so a controller knows how hard its throttles really push.
    pub thrust_scale: f32,
}

impl Default for CarState {
    /// A car at rest on its wheels: what tests and the Off controller can assume.
    fn default() -> CarState {
        CarState {
            gravity_dir_body: [0.0, -1.0, 0.0],
            angular_velocity_body_rad_s: [0.0; 3],
            linear_velocity_world_m_s: [0.0; 3],
            height_m: 0.6,
            wheels_down: 4,
            up_alignment: 1.0,
            thrust_scale: 1.0,
        }
    }
}

/// What the car is, for controllers that plan forces (PD's allocation, SPEC §8).
#[derive(Debug, Clone, PartialEq)]
pub struct CarModel {
    pub mass_kg: f32,
    /// Principal moments of inertia about the body axes X, Y, Z.
    pub principal_inertia_kg_m2: [f32; 3],
    /// Each nozzle's push at full throttle in Realistic mode, in the body frame:
    /// [force x, y, z, torque x, y, z]. The columns of the SPEC's 6×10 matrix `B`.
    pub nozzle_wrenches: [[f32; 6]; NOZZLE_COUNT],
}

/// Reads the car's state now. `thrust_scale` is the mode's scale times the tank gate.
pub(crate) fn read_car_state(
    physics: &PhysicsWorld,
    car: &Car,
    wheels_down: usize,
    thrust_scale: f32,
) -> CarState {
    let body = &physics.bodies[car.chassis];
    let to_body = body.rotation().inverse();
    let world_down = Vector::new(0.0, -1.0, 0.0);
    CarState {
        gravity_dir_body: (to_body * world_down).to_array(),
        angular_velocity_body_rad_s: (to_body * body.angvel()).to_array(),
        linear_velocity_world_m_s: body.linvel().to_array(),
        height_m: height_above_ground_m(physics, car),
        wheels_down,
        up_alignment: car.up_alignment(physics),
        thrust_scale,
    }
}

/// SPEC §9.2: a world-down ray from the centre of mass, ignoring the car itself.
fn height_above_ground_m(physics: &PhysicsWorld, car: &Car) -> f32 {
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
    let mut nozzle_wrenches = [[0.0; 6]; NOZZLE_COUNT];
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
