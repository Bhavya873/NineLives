//! The simulated world: the ground, the car, and the fixed physics step.
//!
//! `World::step` advances exactly one physics step of `PHYSICS_DT_S` (1/120 s).
//! M1 has only flat ground and driving; thrusters, the air tank and events join
//! the step in M3, following the order in SPEC §7.1.
//!
//! `snapshot` and `restore` save and load the whole world as bytes. They power the
//! game's "reset to checkpoint", training checkpoints and the determinism tests.

use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::car::Car;
use crate::conventions::{NOZZLE_COUNT, PHYSICS_DT_S, PHYSICS_HZ, WHEEL_COUNT, gravity_vector};
use crate::input::InputFrame;
use crate::vehicle_config::VehicleConfig;

/// Ground friction for flat-ground worlds when nothing else is chosen (SPEC §7.2).
/// It only matters when the car's body scrapes the ground.
pub const DEFAULT_GROUND_FRICTION: f32 = 1.0;

/// Half-size of the flat ground box, in metres. Big enough that M1 tests never
/// drive off it. Course pieces replace this in M7.
const FLAT_GROUND_HALF_EXTENTS_M: [f32; 3] = [2000.0, 0.5, 2000.0];

/// Where the car starts on flat ground: centre of mass a little above its resting
/// height (about 0.6 m) so it settles onto its wheels.
const FLAT_GROUND_START_HEIGHT_M: f32 = 0.7;

/// Where something is and which way it faces, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Centre position.
    pub position_m: [f32; 3],
    /// Orientation as a quaternion (x, y, z, w), the order three.js uses.
    pub rotation_xyzw: [f32; 4],
}

impl Pose {
    /// Builds a pose from Rapier's position vector and rotation quaternion.
    pub fn new(position_m: Vector, rotation: Rotation) -> Pose {
        Pose {
            position_m: position_m.to_array(),
            rotation_xyzw: [rotation.x, rotation.y, rotation.z, rotation.w],
        }
    }
}

/// Version of the snapshot byte format. Bump it whenever `World`, `Car` or anything
/// they contain changes shape, so old snapshots are refused instead of misread.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 2;

/// Why a snapshot could not be made or loaded.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("world snapshot could not be encoded or decoded: {0}")]
    Encoding(#[from] postcard::Error),

    #[error("world snapshot has format version {found}, but this build reads version {expected}")]
    WrongVersion { found: u32, expected: u32 },

    #[error("world snapshot has {extra_bytes} unexpected bytes after the end")]
    TrailingBytes { extra_bytes: usize },
}

/// A world with flat ground and one car.
#[derive(Serialize, Deserialize)]
pub struct World {
    physics: PhysicsWorld,
    car: Car,
    /// The throttles the controller asked for last; held until it asks again (SPEC §7.1).
    controller_throttles: [f32; NOZZLE_COUNT],
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
            controller_throttles: [0.0; NOZZLE_COUNT],
            step_count: 0,
        }
    }

    /// Advances the world by one physics step (1/120 s).
    pub fn step(&mut self, input: InputFrame) {
        let commanded_throttles = self.controller_throttles;
        self.car
            .prepare_physics_step(&mut self.physics, input, commanded_throttles);
        self.physics.step();
        self.step_count += 1;
    }

    /// Sets the throttles (0..1 per nozzle, in nozzle order) that the controller wants.
    /// They are held until the next call. Controllers run at 60 Hz, so callers set
    /// them every `PHYSICS_STEPS_PER_CONTROL_STEP` physics steps (SPEC §7.1).
    pub fn set_controller_throttles(&mut self, throttles: [f32; NOZZLE_COUNT]) {
        self.controller_throttles = throttles;
    }

    /// How open each thruster valve is now, 0..1, in nozzle order. The renderer sizes
    /// the flames from these.
    pub fn effective_throttles(&self) -> [f32; NOZZLE_COUNT] {
        self.car.effective_throttles()
    }

    /// Moves the car to a new position, orientation and velocity, for tests and for
    /// training scenarios (SPEC §9.6). Velocities are in world coordinates.
    pub fn place_car(
        &mut self,
        position_m: [f32; 3],
        rotation_xyzw: [f32; 4],
        linear_velocity_m_s: [f32; 3],
        angular_velocity_rad_s: [f32; 3],
    ) {
        let [x, y, z, w] = rotation_xyzw;
        let rotation = Rotation::from_xyzw(x, y, z, w).normalize();
        let body = &mut self.physics.bodies[self.car.chassis];
        body.set_position(
            Pose3::from_parts(Vector::from_array(position_m), rotation),
            true,
        );
        body.set_linvel(Vector::from_array(linear_velocity_m_s), true);
        body.set_angvel(Vector::from_array(angular_velocity_rad_s), true);
        // Refresh the scene-query tree so the wheel rays see the car's new place.
        self.physics.detect_collisions(&(), &());
    }

    /// Saves the complete state of the world as bytes: the format version, then the world.
    pub fn snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        Ok(postcard::to_allocvec(&(SNAPSHOT_FORMAT_VERSION, self))?)
    }

    /// Replaces this world with one saved by `snapshot`. Stepping the restored world
    /// with the same inputs gives bit-identical results to the original.
    /// On any error this world is left unchanged.
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
        let (version, after_version): (u32, &[u8]) = postcard::take_from_bytes(snapshot)?;
        if version != SNAPSHOT_FORMAT_VERSION {
            return Err(SnapshotError::WrongVersion {
                found: version,
                expected: SNAPSHOT_FORMAT_VERSION,
            });
        }
        let (world, leftover): (World, &[u8]) = postcard::take_from_bytes(after_version)?;
        if !leftover.is_empty() {
            return Err(SnapshotError::TrailingBytes {
                extra_bytes: leftover.len(),
            });
        }
        *self = world;
        Ok(())
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

    /// The car's current position and orientation (its centre of mass).
    pub fn chassis_pose(&self) -> Pose {
        let body = &self.physics.bodies[self.car.chassis];
        Pose::new(body.translation(), *body.rotation())
    }

    /// Where each wheel is drawn, in wheel order FL, FR, RL, RR. Includes steering
    /// and how far each wheel has rolled, so the renderer can spin them.
    pub fn wheel_poses(&self) -> [Pose; WHEEL_COUNT] {
        self.car.wheel_poses(&self.physics)
    }

    /// The car's velocity in world coordinates.
    pub fn linear_velocity_m_s(&self) -> [f32; 3] {
        self.physics.bodies[self.car.chassis].linvel().to_array()
    }

    /// The car's angular velocity in world coordinates.
    pub fn angular_velocity_rad_s(&self) -> [f32; 3] {
        self.physics.bodies[self.car.chassis].angvel().to_array()
    }

    /// The car's angular velocity in its own body frame: [roll rate about +X,
    /// yaw rate about +Y, pitch rate about +Z] (SPEC §4 signs).
    pub fn angular_velocity_body_rad_s(&self) -> [f32; 3] {
        let body = &self.physics.bodies[self.car.chassis];
        let world_rate = body.angvel();
        (body.rotation().inverse() * world_rate).to_array()
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
