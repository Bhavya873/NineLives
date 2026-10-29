//! The car: a rigid body with two box colliders and four raycast wheels (SPEC §7.3).
//!
//! The wheels are not physical objects. Each one is a ray cast downwards from its
//! suspension mount; Rapier's `DynamicRayCastVehicleController` turns the hits into
//! spring, grip, engine and brake forces on the chassis. This module builds the car
//! and turns the driver's `InputFrame` into those wheel settings.

use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

use crate::conventions::{NOZZLE_COUNT, PHYSICS_DT_S, WHEEL_COUNT};
use crate::input::InputFrame;
use crate::thrusters::{Valves, apply_nozzle_forces};
use crate::vehicle_config::{DriveConfig, SteeringConfig, VehicleConfig};
use crate::world::Pose;

/// Index of the front-left wheel. The front wheels (0 and 1) steer.
const FRONT_LEFT_WHEEL: usize = 0;
/// Index of the front-right wheel.
const FRONT_RIGHT_WHEEL: usize = 1;

/// The car's handles into the physics world, plus its driving state.
///
/// It is saved inside a `World` snapshot together with the physics world those
/// handles point into; on its own a `Car` means nothing.
#[derive(Serialize, Deserialize)]
pub struct Car {
    /// The chassis rigid body.
    pub chassis: RigidBodyHandle,
    /// The main body box.
    pub body_collider: ColliderHandle,
    /// The thin roof box on top of the body.
    pub roof_collider: ColliderHandle,
    wheels: DynamicRayCastVehicleController,
    /// Current front-wheel angle. Positive turns left.
    steering_angle_rad: f32,
    /// The thruster valves and how open they are.
    valves: Valves,
    config: VehicleConfig,
}

impl Car {
    /// Builds the car in `physics` with its centre of mass at `position_m`, facing
    /// `yaw_rad` (0 faces world +X; positive turns the nose left).
    pub fn spawn(
        physics: &mut PhysicsWorld,
        config: &VehicleConfig,
        position_m: Vector,
        yaw_rad: f32,
    ) -> Car {
        let chassis_body = build_chassis_body(config, position_m, yaw_rad);
        let (chassis, body_collider) = physics.insert(chassis_body, build_body_collider(config));
        let roof_collider = physics.insert_collider(build_roof_collider(config), Some(chassis));

        Car {
            chassis,
            body_collider,
            roof_collider,
            wheels: build_wheels(config, chassis),
            steering_angle_rad: 0.0,
            valves: Valves::default(),
            config: config.clone(),
        }
    }

    /// Everything the car does before the physics step, in SPEC §7.1 order: read the
    /// driver's input, move the valves toward the commanded throttles, reset and add
    /// forces (drag, nozzles), then run the raycast wheels.
    /// `thrust_multiplier` is the mode's thrust scale times the tank's thrust gate;
    /// `wind_m_s` is the wind now, in world coordinates.
    /// Returns the total thrust applied this step, for the air tank.
    /// Call exactly once per physics step, just before `PhysicsWorld::step`.
    pub fn prepare_physics_step(
        &mut self,
        physics: &mut PhysicsWorld,
        input: InputFrame,
        commanded_throttles: [f32; NOZZLE_COUNT],
        thrust_multiplier: f32,
        wind_m_s: [f32; 3],
    ) -> f32 {
        self.apply_driver_input(physics, input);
        let thrusters = &self.config.thrusters;
        self.valves.update(
            commanded_throttles,
            PHYSICS_DT_S,
            thrusters.valve_time_constant_s,
        );
        self.reset_forces_and_apply_air_drag(physics, Vector::from_array(wind_m_s));
        let thrust_n = apply_nozzle_forces(
            &mut physics.bodies[self.chassis],
            thrusters,
            self.valves.effective_throttles(),
            thrust_multiplier,
        );
        self.update_wheels(physics);
        thrust_n
    }

    /// Closes every valve and centres the steering, e.g. when the car is moved to
    /// start a new run.
    pub fn reset_actuators(&mut self) {
        self.valves.close_all();
        self.steering_angle_rad = 0.0;
        for wheel in self.wheels.wheels_mut() {
            wheel.steering = 0.0;
        }
    }

    /// Throws away what the wheel rays last saw (contact, suspension length, wheel spin),
    /// for when the car is moved: those rays were cast at the old pose. The next physics
    /// step casts them again.
    pub fn forget_wheel_rays(&mut self) {
        self.wheels = build_wheels(&self.config, self.chassis);
    }

    /// The raycast wheels, read-only (for `diagnostics`).
    pub(crate) fn wheels(&self) -> &[rapier3d::control::Wheel] {
        self.wheels.wheels()
    }

    /// The car's settings.
    pub fn config(&self) -> &VehicleConfig {
        &self.config
    }

    /// Exhaust speed of the cold gas, from the vehicle config.
    pub fn exhaust_speed_m_s(&self) -> f32 {
        self.config.thrusters.exhaust_speed_m_s
    }

    /// How open each thruster valve is now, 0..1, in nozzle order.
    pub fn effective_throttles(&self) -> [f32; NOZZLE_COUNT] {
        self.valves.effective_throttles()
    }

    /// A copy of the valves, for the actuator predictor.
    pub(crate) fn valves(&self) -> Valves {
        self.valves
    }

    /// Sets steering, engine force and brakes from the driver's input.
    fn apply_driver_input(&mut self, physics: &PhysicsWorld, input: InputFrame) {
        let forward_speed_m_s = self.forward_speed_m_s(physics);
        self.update_steering(input.steer_fraction(), forward_speed_m_s);

        let (engine_force_n, brake_impulse_n_s) =
            self.engine_and_brake(input.throttle_fraction(), forward_speed_m_s);
        for wheel in self.wheels.wheels_mut() {
            wheel.engine_force = engine_force_n;
            wheel.brake = brake_impulse_n_s;
        }
    }

    /// Clears last step's forces and torques, then adds air drag at the centre of
    /// pressure (SPEC §7.2), against the car's motion through `wind_m_s` (world frame).
    /// Rapier keeps user forces between steps, so this must run once per physics step,
    /// before adding any other force.
    fn reset_forces_and_apply_air_drag(&self, physics: &mut PhysicsWorld, wind_m_s: Vector) {
        let drag = &self.config.air_drag;
        let body = &mut physics.bodies[self.chassis];
        let rotation = *body.rotation();
        // Drag acts on the speed through the air, not over the ground, and each body
        // axis has its own drag area, so work in the body frame.
        let air_velocity_body_m_s = rotation.inverse() * (body.linvel() - wind_m_s);
        let areas_m2 = Vector::from_array(drag.drag_area_m2);
        // SPEC §7.2: F_i = -0.5 · ρ · CdA_i · |v| · v_i along each body axis i.
        let drag_body_n = -0.5
            * drag.air_density_kg_m3
            * air_velocity_body_m_s.length()
            * (areas_m2 * air_velocity_body_m_s);
        let offset_m = rotation * Vector::from_array(drag.center_of_pressure_offset_m);
        let centre_of_pressure_m = body.center_of_mass() + offset_m;
        body.reset_forces(false);
        body.reset_torques(false);
        body.add_force_at_point(rotation * drag_body_n, centre_of_pressure_m, false);
    }

    /// Casts the wheel rays and applies suspension, grip, engine and brake forces.
    ///
    /// Skipped while the car is upside down: its "down" rays then point at the sky, and
    /// anything they hit (a ceiling, the underside of a ramp) must not push the car.
    fn update_wheels(&mut self, physics: &mut PhysicsWorld) {
        if !self.is_upright(physics) {
            return;
        }
        // The rays start inside the car, so they must ignore the car's own colliders.
        let filter = QueryFilter::default().exclude_rigid_body(self.chassis);
        let queries = physics.broad_phase.as_query_pipeline_mut(
            physics.narrow_phase.query_dispatcher(),
            &mut physics.bodies,
            &mut physics.colliders,
            filter,
        );
        self.wheels.update_vehicle(PHYSICS_DT_S, queries);
    }

    /// Speed along the car's nose direction. Negative when rolling backwards.
    pub fn forward_speed_m_s(&self, physics: &PhysicsWorld) -> f32 {
        let body = &physics.bodies[self.chassis];
        let forward = body.rotation() * Vector::X;
        forward.dot(body.linvel())
    }

    /// Which wheels touch the ground, in wheel order FL, FR, RL, RR.
    ///
    /// These come from the rays cast in the last `update_wheels` call, which happens
    /// just before the physics step, so they lag the chassis pose by one physics step
    /// (1/120 s). A wheel only counts when the car is the right way up, because on an
    /// upside-down car the "down" rays point at the sky (SPEC §7.3).
    pub fn wheels_touching(&self, physics: &PhysicsWorld) -> [bool; WHEEL_COUNT] {
        let mut touching = [false; WHEEL_COUNT];
        if !self.is_upright(physics) {
            return touching;
        }
        for (index, wheel) in self.wheels.wheels().iter().enumerate() {
            touching[index] = wheel.raycast_info().is_in_contact;
        }
        touching
    }

    /// How far each wheel's spring is pushed in, 0 (fully out, or not touching) to 1
    /// (at the end of its travel), in wheel order FL, FR, RL, RR (SPEC §9.2 rows 14–17).
    /// Like `wheels_touching`, it comes from the last wheel rays.
    pub fn suspension_compressions(&self, physics: &PhysicsWorld) -> [f32; WHEEL_COUNT] {
        self.compressions_of(self.wheels_touching(physics))
    }

    /// `suspension_compressions` for wheels already known to be `touching` (from
    /// `wheels_touching` this step), so the caller need not check contact twice.
    pub(crate) fn compressions_of(&self, touching: [bool; WHEEL_COUNT]) -> [f32; WHEEL_COUNT] {
        let mut compressions = [0.0; WHEEL_COUNT];
        for (index, wheel) in self.wheels.wheels().iter().enumerate() {
            if !touching[index] {
                continue;
            }
            let pushed_in_m = wheel.suspension_rest_length - wheel.raycast_info().suspension_length;
            let travel_m = wheel.max_suspension_travel;
            compressions[index] = (pushed_in_m / travel_m).clamp(0.0, 1.0);
        }
        compressions
    }

    /// True when the car's up axis points at least a little upwards. Below that, the
    /// wheel rays point at the sky and must be ignored (SPEC §7.3).
    fn is_upright(&self, physics: &PhysicsWorld) -> bool {
        self.up_alignment(physics) > 0.0
    }

    /// How much the car's up axis points along world up: 1 upright, 0 on its side,
    /// -1 upside down.
    pub fn up_alignment(&self, physics: &PhysicsWorld) -> f32 {
        let body = &physics.bodies[self.chassis];
        (body.rotation() * Vector::Y).dot(Vector::Y)
    }

    /// Where each wheel is drawn, in wheel order FL, FR, RL, RR.
    ///
    /// Built from the chassis pose *now*, so the wheels never lag or detach from the
    /// body. A wheel's centre sits at the end of its suspension: the length its ray
    /// measured when it touches the ground, otherwise the rest length (in the air,
    /// upside down, or before the first step). Its rotation is the chassis rotation, then the steering
    /// angle about the body's up axis, then the roll about the axle (+Z). Rolling
    /// forward spins a wheel about -Z, so the roll angle is negated.
    pub fn wheel_poses(&self, physics: &PhysicsWorld) -> [Pose; WHEEL_COUNT] {
        let chassis_pose = physics.bodies[self.chassis].position();
        let touching = self.wheels_touching(physics);
        let mut poses = [Pose::new(Vector::ZERO, Rotation::IDENTITY); WHEEL_COUNT];
        for (index, wheel) in self.wheels.wheels().iter().enumerate() {
            let suspension_length_m = if touching[index] {
                wheel.raycast_info().suspension_length
            } else {
                wheel.suspension_rest_length
            };
            let mount_m = chassis_pose * wheel.chassis_connection_point_cs;
            let down = chassis_pose.rotation * wheel.direction_cs;
            let centre_m = mount_m + down * suspension_length_m;

            let steer = Rotation::from_rotation_y(wheel.steering);
            // Wrap the ever-growing roll angle so f32 keeps its precision on long drives.
            let roll = Rotation::from_rotation_z(-wheel.rotation.rem_euclid(TAU));
            poses[index] = Pose::new(centre_m, chassis_pose.rotation * steer * roll);
        }
        poses
    }

    /// Current front-wheel angle. Positive turns left.
    pub fn steering_angle_rad(&self) -> f32 {
        self.steering_angle_rad
    }

    /// Moves the front wheels toward the driver's requested angle, no faster than
    /// the steering rate limit, and never past the speed-dependent angle limit.
    fn update_steering(&mut self, steer_fraction: f32, forward_speed_m_s: f32) {
        let steering = &self.config.steering;
        let limit_rad = steering_limit_rad(steering, forward_speed_m_s.abs());
        let target_rad = steer_fraction * limit_rad;
        let max_change_rad = steering.max_rate_rad_s * PHYSICS_DT_S;
        let change_rad =
            (target_rad - self.steering_angle_rad).clamp(-max_change_rad, max_change_rad);
        self.steering_angle_rad += change_rad;

        let wheels = self.wheels.wheels_mut();
        wheels[FRONT_LEFT_WHEEL].steering = self.steering_angle_rad;
        wheels[FRONT_RIGHT_WHEEL].steering = self.steering_angle_rad;
    }

    /// Works out the per-wheel engine force and brake impulse for a throttle in
    /// -1..1. Pulling back brakes while rolling forward, and reverses once nearly stopped.
    fn engine_and_brake(&self, throttle_fraction: f32, forward_speed_m_s: f32) -> (f32, f32) {
        let drive = &self.config.drive;
        let full_engine_n = engine_force_limit_n(drive, forward_speed_m_s);

        if throttle_fraction > 0.0 {
            if forward_speed_m_s >= drive.max_speed_m_s {
                return (0.0, 0.0);
            }
            return (throttle_fraction * full_engine_n, 0.0);
        }

        if throttle_fraction < 0.0 {
            if forward_speed_m_s > drive.reverse_below_speed_m_s {
                let brake_n_s = -throttle_fraction * drive.brake_impulse_per_wheel_n_s;
                return (0.0, brake_n_s);
            }
            if forward_speed_m_s <= -drive.max_reverse_speed_m_s {
                return (0.0, 0.0);
            }
            return (throttle_fraction * full_engine_n, 0.0);
        }

        (0.0, drive.rolling_resistance_impulse_per_wheel_n_s)
    }
}

/// The most push one wheel's engine can give at `speed_m_s` (SPEC §7.3): the force
/// limit at low speed, then the power shared over four wheels divided by the speed.
pub(crate) fn engine_force_limit_n(drive: &DriveConfig, speed_m_s: f32) -> f32 {
    let power_limited_n = drive.max_engine_power_w / (WHEEL_COUNT as f32 * speed_m_s.abs());
    drive.max_engine_force_per_wheel_n.min(power_limited_n)
}

/// The largest steering angle allowed at `speed_m_s`: the low-speed limit when
/// stopped, sliding linearly down to the high-speed limit at `high_speed_m_s` and
/// above (SPEC §7.3).
pub(crate) fn steering_limit_rad(steering: &SteeringConfig, speed_m_s: f32) -> f32 {
    let low_rad = steering.max_angle_low_speed_rad;
    let high_rad = steering.max_angle_high_speed_rad;
    let progress = (speed_m_s / steering.high_speed_m_s).clamp(0.0, 1.0);
    low_rad + (high_rad - low_rad) * progress
}

/// The chassis rigid body: mass, inertia and centre of mass are set by hand (SPEC §7.3),
/// so the colliders below have zero density and add no mass.
fn build_chassis_body(
    config: &VehicleConfig,
    position_m: Vector,
    yaw_rad: f32,
) -> RigidBodyBuilder {
    let chassis = &config.chassis;
    let inertia = Vector::from_array(chassis.inertia_kg_m2);
    let centre_of_mass_m = Vector::from_array(chassis.center_of_mass_offset_m);
    let mass_properties = MassProperties::new(centre_of_mass_m, chassis.dry_mass_kg, inertia);
    RigidBodyBuilder::dynamic()
        .translation(position_m)
        .rotation(Vector::new(0.0, yaw_rad, 0.0))
        .additional_mass_properties(mass_properties)
        .can_sleep(false)
}

/// The main body box, centred on the centre of mass, with its bottom front and rear
/// edges cut away by `chassis.underside_chamfer_m` so the nose and tail ride over a
/// curve's rise instead of digging in.
fn build_body_collider(config: &VehicleConfig) -> ColliderBuilder {
    let [half_x, half_y, half_z] = config.chassis.half_extents_m;
    let [chamfer_length_m, chamfer_rise_m] = config.chassis.underside_chamfer_m;
    let plain_box = ColliderBuilder::cuboid(half_x, half_y, half_z);
    if chamfer_length_m <= 0.0 || chamfer_rise_m <= 0.0 {
        return plain_box.density(0.0);
    }
    let mut corners = Vec::new();
    for z in [-half_z, half_z] {
        for x in [-half_x, half_x] {
            corners.push(Vector::new(x, half_y, z));
            corners.push(Vector::new(x, -half_y + chamfer_rise_m, z));
        }
        for x in [-half_x + chamfer_length_m, half_x - chamfer_length_m] {
            corners.push(Vector::new(x, -half_y, z));
        }
    }
    // A cut box is always convex; the plain box only covers a degenerate hull.
    ColliderBuilder::convex_hull(&corners)
        .unwrap_or(plain_box)
        .density(0.0)
}

/// The thin roof box, sitting on top of the main body box.
fn build_roof_collider(config: &VehicleConfig) -> ColliderBuilder {
    let body_half_height_m = config.chassis.half_extents_m[1];
    let [half_x, half_y, half_z] = config.chassis.roof_half_extents_m;
    let roof_centre_m = Vector::new(0.0, body_half_height_m + half_y, 0.0);
    ColliderBuilder::cuboid(half_x, half_y, half_z)
        .translation(roof_centre_m)
        .density(0.0)
}

/// The four raycast wheels: rays point down the body's -Y, axles along +Z.
fn build_wheels(
    config: &VehicleConfig,
    chassis: RigidBodyHandle,
) -> DynamicRayCastVehicleController {
    let wheels = &config.wheels;
    let tuning = WheelTuning {
        suspension_stiffness: wheels.suspension_stiffness_per_kg,
        suspension_compression: wheels.suspension_compression_damping_per_kg,
        suspension_damping: wheels.suspension_relaxation_damping_per_kg,
        max_suspension_travel: wheels.suspension_max_travel_m,
        side_friction_stiffness: wheels.side_friction_stiffness,
        friction_slip: wheels.friction_slip,
        max_suspension_force: wheels.max_suspension_force_n,
    };

    let mut controller = DynamicRayCastVehicleController::new(chassis);
    for mount_point in wheels.mount_points_m {
        controller.add_wheel(
            Vector::from_array(mount_point),
            -Vector::Y,
            Vector::Z,
            wheels.suspension_rest_length_m,
            wheels.radius_m,
            &tuning,
        );
    }
    controller
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steering_config() -> SteeringConfig {
        SteeringConfig {
            max_angle_low_speed_rad: 0.5,
            max_angle_high_speed_rad: 0.15,
            high_speed_m_s: 40.0,
            max_rate_rad_s: 2.0,
        }
    }

    #[test]
    fn steering_limit_is_the_low_speed_value_when_stopped() {
        assert_eq!(steering_limit_rad(&steering_config(), 0.0), 0.5);
    }

    #[test]
    fn steering_limit_is_the_high_speed_value_at_and_above_high_speed() {
        assert_eq!(steering_limit_rad(&steering_config(), 40.0), 0.15);
        assert_eq!(steering_limit_rad(&steering_config(), 90.0), 0.15);
    }

    fn drive_config() -> DriveConfig {
        DriveConfig {
            max_engine_force_per_wheel_n: 3000.0,
            max_engine_power_w: 240_000.0,
            max_speed_m_s: 55.0,
            boost_max_speed_m_s: 60.0,
            boost_fade_m_s: 10.0,
            max_reverse_speed_m_s: 8.0,
            brake_impulse_per_wheel_n_s: 40.0,
            rolling_resistance_impulse_per_wheel_n_s: 1.5,
            reverse_below_speed_m_s: 0.5,
        }
    }

    #[test]
    fn engine_gives_its_full_force_at_low_speed() {
        assert_eq!(engine_force_limit_n(&drive_config(), 0.0), 3000.0);
        assert_eq!(engine_force_limit_n(&drive_config(), 10.0), 3000.0);
    }

    #[test]
    fn engine_force_is_power_over_speed_at_high_speed() {
        // 240 kW over four wheels at 40 m/s: 60 kW / 40 m/s = 1.5 kN per wheel.
        assert_eq!(engine_force_limit_n(&drive_config(), 40.0), 1500.0);
        assert_eq!(engine_force_limit_n(&drive_config(), -40.0), 1500.0);
    }
}
