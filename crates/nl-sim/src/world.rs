//! The simulated world: the ground, the car, and the fixed physics step.
//!
//! `World::step` advances exactly one physics step of `PHYSICS_DT_S` (1/120 s).
//! M1 has only flat ground and driving; thrusters, the air tank and events join
//! the step in M3, following the order in SPEC §7.1.

use rapier3d::prelude::*;

use crate::car::Car;
use crate::conventions::{PHYSICS_DT_S, PHYSICS_HZ, WHEEL_COUNT, gravity_vector};
use crate::input::InputFrame;
use crate::vehicle_config::VehicleConfig;

/// Half-size of the flat ground box, in metres. Big enough that M1 tests never
/// drive off it. Course pieces replace this in M7.
const FLAT_GROUND_HALF_EXTENTS_M: [f32; 3] = [2000.0, 0.5, 2000.0];

/// Where the car starts on flat ground: centre of mass a little above its resting
/// height (about 0.6 m) so it settles onto its wheels.
const FLAT_GROUND_START_HEIGHT_M: f32 = 0.7;

/// The car's position and orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChassisPose {
    /// Centre of mass in world coordinates.
    pub position_m: [f32; 3],
    /// Orientation as a quaternion (x, y, z, w), the order three.js uses.
    pub rotation_xyzw: [f32; 4],
}

/// A world with flat ground and one car.
pub struct World {
    physics: PhysicsWorld,
    car: Car,
    step_count: u64,
}

impl World {
    /// Builds flat ground and puts the car at the origin, facing world +X.
    ///
    /// `ground_friction` applies when the car's body scrapes the ground. Tyre grip
    /// comes from `wheels.friction_slip` in the vehicle config instead, because
    /// Rapier's raycast wheels do not read collider friction.
    pub fn new_on_flat_ground(vehicle_config: &VehicleConfig, ground_friction: f32) -> World {
        let mut physics = PhysicsWorld::new();
        physics.gravity = gravity_vector();
        physics.integration_parameters.dt = PHYSICS_DT_S;
        physics.insert_collider(build_flat_ground(ground_friction), None);

        let start_m = Vector::new(0.0, FLAT_GROUND_START_HEIGHT_M, 0.0);
        let car = Car::spawn(&mut physics, vehicle_config, start_m, 0.0);
        // Fill the scene-query tree now; otherwise the wheel rays on the very first
        // step would find nothing, because Rapier only refreshes it inside a step.
        physics.detect_collisions(&(), &());

        World {
            physics,
            car,
            step_count: 0,
        }
    }

    /// Advances the world by one physics step (1/120 s).
    pub fn step(&mut self, input: InputFrame) {
        // SPEC §7.1 order, for the parts that exist so far:
        // 1. read driver input, 7. forces, 8. vehicle controller, 9. physics step.
        self.car.apply_driver_input(&self.physics, input);
        self.car.reset_forces_and_apply_air_drag(&mut self.physics);
        self.car.update_wheels(&mut self.physics);
        self.physics.step();
        self.step_count += 1;
    }

    /// Number of physics steps taken since the world was built.
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Simulated time since the world was built.
    pub fn time_s(&self) -> f64 {
        // Divide by the exact rate; PHYSICS_DT_S is a rounded f32.
        self.step_count as f64 / PHYSICS_HZ as f64
    }

    /// The car's current position and orientation.
    pub fn chassis_pose(&self) -> ChassisPose {
        let body = &self.physics.bodies[self.car.chassis];
        let rotation = body.rotation();
        ChassisPose {
            position_m: body.translation().to_array(),
            rotation_xyzw: [rotation.x, rotation.y, rotation.z, rotation.w],
        }
    }

    /// The car's velocity in world coordinates.
    pub fn linear_velocity_m_s(&self) -> [f32; 3] {
        self.physics.bodies[self.car.chassis].linvel().to_array()
    }

    /// The car's angular velocity in world coordinates.
    pub fn angular_velocity_rad_s(&self) -> [f32; 3] {
        self.physics.bodies[self.car.chassis].angvel().to_array()
    }

    /// Speed along the car's nose direction. Negative when rolling backwards.
    pub fn forward_speed_m_s(&self) -> f32 {
        self.car.forward_speed_m_s(&self.physics)
    }

    /// Which wheels touch the ground, in wheel order FL, FR, RL, RR.
    pub fn wheels_touching(&self) -> [bool; WHEEL_COUNT] {
        self.car.wheels_touching(&self.physics)
    }

    /// Current front-wheel angle. Positive turns left.
    pub fn steering_angle_rad(&self) -> f32 {
        self.car.steering_angle_rad()
    }
}

/// A large flat box whose top surface is at y = 0.
fn build_flat_ground(friction: f32) -> ColliderBuilder {
    let [half_x, half_y, half_z] = FLAT_GROUND_HALF_EXTENTS_M;
    ColliderBuilder::cuboid(half_x, half_y, half_z)
        .translation(Vector::new(0.0, -half_y, 0.0))
        .friction(friction)
}
