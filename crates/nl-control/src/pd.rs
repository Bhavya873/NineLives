//! The PD baseline controller (SPEC §8): turn the car upright with attitude torque, brake
//! a fast fall early enough to stop before landing, damp sideways sliding before
//! touchdown, and roll back onto the wheels when lying on the ground (the side boosters
//! pushing too). It decides a wanted force and torque, then the allocation picks throttles.

use nl_sim::car_state::{CarModel, CarState, WRENCH_SIZE, Wrench};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT};
use nl_sim::modes::ModeParams;
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

use crate::allocation::Allocator;
use crate::config::{ControllersConfig, PdConfig};
use crate::{ControlCtx, Controller};

/// Below this size the tilt error has no reliable direction (the car is almost exactly
/// upside down), so PD picks a roll instead.
const MIN_TILT_ERROR: f32 = 0.1;

/// The PD controller with its allocation, for one car.
#[derive(Debug, Clone, PartialEq)]
pub struct Pd {
    config: PdConfig,
    mass_kg: f32,
    inertia_kg_m2: [f32; 3],
    /// Upward push of every nozzle at full throttle in a ×1 mode, along body +Y, in N.
    full_lift_n: f32,
    allocator: Allocator,
    /// Last throttles, the warm start for the next allocation.
    throttles: [f32; NOZZLE_COUNT],
}

impl Pd {
    /// A PD controller for the car described by `model`.
    pub fn new(model: &CarModel, config: &ControllersConfig) -> Pd {
        Pd {
            config: config.pd,
            mass_kg: model.mass_kg,
            inertia_kg_m2: model.principal_inertia_kg_m2,
            full_lift_n: full_lift_n(&model.nozzle_wrenches),
            allocator: Allocator::new(&model.nozzle_wrenches, &config.allocation),
            throttles: [0.0; NOZZLE_COUNT],
        }
    }

    /// A PD controller for the car `vehicle` describes, in `mode` (reads its mass,
    /// inertia and nozzle pushes from a throwaway world).
    pub fn for_vehicle(
        vehicle: &VehicleConfig,
        mode: ModeParams,
        config: &ControllersConfig,
    ) -> Pd {
        let world = World::new_on_flat_ground(vehicle, mode, DEFAULT_GROUND_FRICTION, 0);
        Pd::new(&world.car_model(), config)
    }

    /// The force and torque PD wants now, in the body frame:
    /// [force x, y, z, torque x, y, z]. All zero while the car drives on its wheels,
    /// to save air.
    pub fn wanted_wrench(&self, state: &CarState) -> Wrench {
        if state.on_wheels() {
            return [0.0; WRENCH_SIZE];
        }
        let mut torque = self.attitude_torque(state);
        // Low, tipped over and nearly still: lying on the ground, not falling past it.
        let lying_on_ground = state.height_m < self.config.on_ground_height_m
            && state.up_alignment() < self.config.upright_alignment
            && state.falling_speed_m_s().abs() < self.config.lying_max_fall_speed_m_s;
        let mut force = self.braking_force_body(state);
        if lying_on_ground {
            // SPEC §8: roll torque only, to flip back onto the wheels. Lifting the car
            // over its side edge takes far more torque than turning it in the air.
            torque[0] *= self.config.ground_roll_gain;
            torque[1] = 0.0;
            torque[2] = 0.0;
            force[2] = self.side_push_n(state);
        } else {
            force[2] = self.lateral_damping_n(state);
        }
        [
            force[0], force[1], force[2], torque[0], torque[1], torque[2],
        ]
    }

    /// SPEC §8: up_des = −gravity_dir_body; error e = (0, 1, 0) × up_des;
    /// τ = I · (kp·e − kd·ω), per body axis.
    fn attitude_torque(&self, state: &CarState) -> [f32; 3] {
        let [gx, gy, gz] = state.gravity_dir_body;
        let up_wanted = [-gx, -gy, -gz];
        // (0, 1, 0) × (x, y, z) = (z, 0, −x).
        let mut error = [up_wanted[2], 0.0, -up_wanted[0]];
        let upside_down = up_wanted[1] < 0.0;
        if upside_down && error[0].abs() < MIN_TILT_ERROR && error[2].abs() < MIN_TILT_ERROR {
            // The cross product vanishes; roll (either way works) at the threshold size,
            // so the torque does not jump as the error crosses it.
            let roll_sign = if up_wanted[2] >= 0.0 { 1.0 } else { -1.0 };
            error = [roll_sign * MIN_TILT_ERROR, 0.0, 0.0];
        }
        let spin = state.angular_velocity_body_rad_s;
        let mut torque = [0.0; 3];
        for axis in 0..3 {
            let wanted = self.config.kp * error[axis] - self.config.kd * spin[axis];
            torque[axis] = self.inertia_kg_m2[axis] * wanted;
        }
        torque
    }

    /// SPEC §8: when airborne, roughly upright, falling fast and low enough to need it
    /// (`should_brake`), push up with F = m·(g + kv·(v_down − target)) along the body's
    /// +Y (w = [0, F_y, 0, …]), which only the lift nozzles provide; otherwise nothing, to
    /// save air.
    fn braking_force_body(&self, state: &CarState) -> [f32; 3] {
        if !self.should_brake(state) {
            return [0.0; 3];
        }
        let falling_m_s = state.falling_speed_m_s();
        let push_n = self.mass_kg
            * (GRAVITY_M_S2 + self.config.kv * (falling_m_s - self.config.target_descent_m_s));
        [0.0, push_n, 0.0]
    }

    /// Airborne, upright enough, falling faster than `brake_speed_m_s`, and at or below
    /// the height where braking must start (`brake_start_height_m`).
    fn should_brake(&self, state: &CarState) -> bool {
        state.wheels_down() == 0
            && state.up_alignment() > self.config.upright_alignment
            && state.falling_speed_m_s() > self.config.brake_speed_m_s
            && state.height_m < self.brake_start_height_m(state)
    }

    /// How high braking must start to slow the fall to `target_descent_m_s` before
    /// touchdown: `brake_safety` × the stopping distance (v² − v_target²) / (2·a) plus
    /// `brake_margin_m`, never below `brake_height_m`. The deceleration a is what
    /// `brake_usable_fraction` of full lift (scaled by the mode) leaves after gravity;
    /// with no lift to spare, braking starts at once.
    fn brake_start_height_m(&self, state: &CarState) -> f32 {
        let config = &self.config;
        let lift_n = self.full_lift_n * state.mode_thrust_scale * config.brake_usable_fraction;
        let deceleration_m_s2 = lift_n / self.mass_kg - GRAVITY_M_S2;
        if deceleration_m_s2 <= 0.0 {
            return f32::INFINITY;
        }
        let falling_m_s = state.falling_speed_m_s();
        let target_m_s = config.target_descent_m_s;
        let speed_squared = (falling_m_s * falling_m_s - target_m_s * target_m_s).max(0.0);
        let stopping_m = speed_squared / (2.0 * deceleration_m_s2);
        (config.brake_safety * stopping_m + config.brake_margin_m).max(config.brake_height_m)
    }

    /// In the air and roughly upright: a sideways force −m·kv_lateral·v_z along body Z,
    /// so the side boosters cancel a slide before touchdown; 0 otherwise.
    fn lateral_damping_n(&self, state: &CarState) -> f32 {
        let damp = state.wheels_down() == 0 && state.up_alignment() > self.config.upright_alignment;
        if !damp {
            return 0.0;
        }
        -self.mass_kg * self.config.kv_lateral * state.linear_velocity_body_m_s[2]
    }

    /// Lying on its side: push along body Z towards world up with `ground_side_push` ×
    /// the car's weight, scaled by how far that side axis points up (world up in the body
    /// frame is −gravity_dir_body).
    fn side_push_n(&self, state: &CarState) -> f32 {
        let up_along_side = -state.gravity_dir_body[2];
        self.config.ground_side_push * self.mass_kg * GRAVITY_M_S2 * up_along_side
    }
}

/// The total upward push (body +Y force) of all nozzles at full throttle, in N.
fn full_lift_n(nozzle_wrenches: &[Wrench; NOZZLE_COUNT]) -> f32 {
    let mut lift_n = 0.0;
    for wrench in nozzle_wrenches {
        lift_n += wrench[1].max(0.0);
    }
    lift_n
}

impl Controller for Pd {
    fn reset(&mut self, _seed: u64) {
        self.throttles = [0.0; NOZZLE_COUNT];
    }

    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        let wrench = self.wanted_wrench(&ctx.state);
        if wrench == [0.0; WRENCH_SIZE] {
            self.throttles = [0.0; NOZZLE_COUNT];
        } else {
            // Plan with the mode's scale only: dividing by the tank gate would drive every
            // nozzle to full throttle as the tank runs dry (SPEC §7.5 fades it instead).
            self.allocator
                .solve(wrench, ctx.state.mode_thrust_scale, &mut self.throttles);
        }
        self.throttles
    }
}
