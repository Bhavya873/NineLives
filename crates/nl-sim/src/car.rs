//! The car: a rigid body with two box colliders and four raycast wheels (SPEC §7.3).
//!
//! The wheels are not physical objects. Each one is a ray cast downwards from its
//! suspension mount; Rapier's `DynamicRayCastVehicleController` turns the hits into
//! spring, grip, engine and brake forces on the chassis. This module builds the car
//! and turns the driver's `InputFrame` into those wheel settings.

use rapier3d::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier3d::prelude::*;

use crate::conventions::{PHYSICS_DT_S, WHEEL_COUNT};
use crate::input::InputFrame;
use crate::vehicle_config::VehicleConfig;

/// Index of the front-left wheel. The front wheels (0 and 1) steer.
const FRONT_LEFT_WHEEL: usize = 0;
/// Index of the front-right wheel.
const FRONT_RIGHT_WHEEL: usize = 1;

/// The car's handles into the physics world, plus its driving state.
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
            config: config.clone(),
        }
    }

    /// Sets steering, engine force and brakes from the driver's input.
    /// Call once per physics step, before `update_wheels`.
    pub fn apply_driver_input(&mut self, physics: &PhysicsWorld, input: InputFrame) {
        let forward_speed_m_s = self.forward_speed_m_s(physics);
        self.update_steering(input.steer_fraction(), forward_speed_m_s);

        let (engine_force_n, brake_impulse_n_s) =
            self.engine_and_brake(input.throttle_fraction(), forward_speed_m_s);
        for wheel in self.wheels.wheels_mut() {
            wheel.engine_force = engine_force_n;
            wheel.brake = brake_impulse_n_s;
        }
    }

    /// Clears last step's forces and torques, then adds air drag at the centre of mass
    /// (SPEC §7.2). Rapier keeps user forces between steps, so this must run once per
    /// physics step, before adding any other force.
    pub fn reset_forces_and_apply_air_drag(&self, physics: &mut PhysicsWorld) {
        let drag = &self.config.air_drag;
        let body = &mut physics.bodies[self.chassis];
        let velocity_m_s = body.linvel();
        // SPEC §7.2: F = -0.5 · ρ · CdA · |v| · v
        let drag_force_n = -0.5
            * drag.air_density_kg_m3
            * drag.drag_area_m2
            * velocity_m_s.length()
            * velocity_m_s;
        body.reset_forces(false);
        body.reset_torques(false);
        body.add_force(drag_force_n, false);
    }

    /// Casts the wheel rays and applies suspension, grip, engine and brake forces.
    ///
    /// Skipped while the car is upside down: its "down" rays then point at the sky, and
    /// anything they hit (a ceiling, the underside of a ramp) must not push the car.
    pub fn update_wheels(&mut self, physics: &mut PhysicsWorld) {
        if self.up_alignment(physics) <= 0.0 {
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
        let up_alignment = self.up_alignment(physics);
        let mut touching = [false; WHEEL_COUNT];
        for (index, wheel) in self.wheels.wheels().iter().enumerate() {
            touching[index] =
                wheel_counts_as_touching(wheel.raycast_info().is_in_contact, up_alignment);
        }
        touching
    }

    /// How much the car's up axis points along world up: 1 upright, 0 on its side,
    /// -1 upside down.
    pub fn up_alignment(&self, physics: &PhysicsWorld) -> f32 {
        let body = &physics.bodies[self.chassis];
        (body.rotation() * Vector::Y).dot(Vector::Y)
    }

    /// Current front-wheel angle. Positive turns left.
    pub fn steering_angle_rad(&self) -> f32 {
        self.steering_angle_rad
    }

    /// Moves the front wheels toward the driver's requested angle, no faster than
    /// the steering rate limit, and never past the speed-dependent angle limit.
    fn update_steering(&mut self, steer_fraction: f32, forward_speed_m_s: f32) {
        let steering = &self.config.steering;
        let limit_rad = steering_limit_rad(
            steering.max_angle_low_speed_rad,
            steering.max_angle_high_speed_rad,
            steering.high_speed_m_s,
            forward_speed_m_s.abs(),
        );
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
        let full_engine_n = drive.max_engine_force_per_wheel_n;

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

/// The largest steering angle allowed at `speed_m_s`: `low_rad` when stopped, sliding
/// linearly down to `high_rad` at `high_speed_m_s` and above (SPEC §7.3).
fn steering_limit_rad(low_rad: f32, high_rad: f32, high_speed_m_s: f32, speed_m_s: f32) -> f32 {
    let progress = (speed_m_s / high_speed_m_s).clamp(0.0, 1.0);
    low_rad + (high_rad - low_rad) * progress
}

/// A wheel touches the ground only if its ray hit something and the car is upright
/// enough (SPEC §7.3). On an upside-down car the rays point up, so a hit means nothing.
fn wheel_counts_as_touching(ray_hit: bool, up_alignment: f32) -> bool {
    ray_hit && up_alignment > 0.0
}

/// The chassis rigid body: mass and inertia are set by hand (SPEC §7.3), so the
/// colliders below have zero density and add no mass.
fn build_chassis_body(
    config: &VehicleConfig,
    position_m: Vector,
    yaw_rad: f32,
) -> RigidBodyBuilder {
    let chassis = &config.chassis;
    let inertia = Vector::from_array(chassis.inertia_kg_m2);
    let mass_properties = MassProperties::new(Vector::ZERO, chassis.dry_mass_kg, inertia);
    RigidBodyBuilder::dynamic()
        .translation(position_m)
        .rotation(Vector::new(0.0, yaw_rad, 0.0))
        .additional_mass_properties(mass_properties)
        .can_sleep(false)
}

/// The main body box, centred on the centre of mass.
fn build_body_collider(config: &VehicleConfig) -> ColliderBuilder {
    let [half_x, half_y, half_z] = config.chassis.half_extents_m;
    ColliderBuilder::cuboid(half_x, half_y, half_z).density(0.0)
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

    #[test]
    fn steering_limit_is_the_low_speed_value_when_stopped() {
        assert_eq!(steering_limit_rad(0.5, 0.15, 40.0, 0.0), 0.5);
    }

    #[test]
    fn steering_limit_is_the_high_speed_value_at_and_above_high_speed() {
        assert_eq!(steering_limit_rad(0.5, 0.15, 40.0, 40.0), 0.15);
        assert_eq!(steering_limit_rad(0.5, 0.15, 40.0, 90.0), 0.15);
    }

    #[test]
    fn a_ray_hit_does_not_count_when_the_car_is_upside_down() {
        assert!(wheel_counts_as_touching(true, 1.0));
        assert!(!wheel_counts_as_touching(true, -1.0));
        assert!(!wheel_counts_as_touching(false, 1.0));
    }
}
