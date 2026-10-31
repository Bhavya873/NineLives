//! The PD baseline controller (SPEC §8): turn the car upright with attitude torque, brake
//! a fast fall early enough to stop before landing, damp sideways sliding before
//! touchdown, and roll back onto the wheels when lying on the ground (the side boosters
//! pushing too). It decides a wanted force and torque, then the allocation picks throttles.

use nl_sim::car_state::{CarModel, CarState, WRENCH_SIZE, Wrench};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT};
use nl_sim::input::InputFrame;
use nl_sim::modes::ModeParams;
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

use nl_sim::actuator_model::ActuatorState;

use crate::allocation::{ActuatorPlan, AllocationDiagnostics, AllocationModel, Allocator};
use crate::config::{ControllersConfig, PdConfig};
use crate::{ControlCtx, Controller};

/// Below this size the tilt error has no reliable direction (the car is almost exactly
/// upside down), so PD picks a roll instead.
const MIN_TILT_ERROR: f32 = 0.1;

/// Wheels that must touch for PD's quiet rule (`driving_on_wheels`) on a calm car.
const QUIET_MIN_WHEELS: usize = 2;

/// The PD controller with its allocation, for one car.
#[derive(Debug, Clone, PartialEq)]
pub struct Pd {
    config: PdConfig,
    mass_kg: f32,
    inertia_kg_m2: [f32; 3],
    /// Upward push of every nozzle at full throttle in a ×1 mode, along body +Y, in N.
    full_lift_n: f32,
    allocator: Allocator,
    /// Which allocation `act` uses (`[allocation] model`).
    allocation_model: AllocationModel,
    /// Last throttles, the warm start for the next allocation.
    throttles: [f32; NOZZLE_COUNT],
    /// The actuator-aware allocation's report on its last solve; None with `legacy`.
    last_allocation: Option<AllocationDiagnostics>,
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
            allocation_model: config.allocation.model,
            throttles: [0.0; NOZZLE_COUNT],
            last_allocation: None,
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

    /// The force and torque PD wants now with no driver input, in the body frame:
    /// [force x, y, z, torque x, y, z] (see `wanted_wrench_for`).
    pub fn wanted_wrench(&self, state: &CarState) -> Wrench {
        self.wanted_wrench_for(state, &InputFrame::default())
    }

    /// The force and torque PD wants now, in the body frame, with the driver's `input`
    /// as air-control wishes (steer turns the heading, throttle tilts the nose). All
    /// zero while the car drives on its wheels, to save air.
    pub fn wanted_wrench_for(&self, state: &CarState, input: &InputFrame) -> Wrench {
        if self.driving_on_wheels(state) {
            return [0.0; WRENCH_SIZE];
        }
        let mut torque = self.attitude_torque(state, &self.air_wish(state, input));
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

    /// On its wheels for PD's purposes: three or more down, or two down while the car is
    /// near level and barely turning, so a wheel lifting on a bump or kerb does not set
    /// the thrusters firing (owner's free-play feedback). A landing that touches first on
    /// one wheel, and hops with every wheel up, still get PD: skipping those cost landings.
    fn driving_on_wheels(&self, state: &CarState) -> bool {
        let config = &self.config;
        let near_level = state.up_alignment() >= libm::cosf(config.quiet_max_tilt_deg.to_radians());
        state.on_wheels()
            || (state.wheels_down() >= QUIET_MIN_WHEELS
                && near_level
                && state.spin_rad_s() <= config.quiet_max_spin_rad_s)
    }

    /// The driver's air-control wish: high enough in the air and roughly upright, steer
    /// asks for a yaw rate (left positive, SPEC §4) and throttle for a pitch (forward
    /// tips the nose down). Near the ground or tumbling, nothing: landing safely wins.
    fn air_wish(&self, state: &CarState, input: &InputFrame) -> AirWish {
        let config = &self.config;
        let free_to_steer = state.wheels_down() == 0
            && state.height_m > config.air_wish_min_height_m
            && state.up_alignment() > config.upright_alignment;
        if !free_to_steer {
            return AirWish::default();
        }
        AirWish {
            yaw_rate_rad_s: input.steer_fraction() * config.air_yaw_rate_rad_s,
            nose_down_rad: input.throttle_fraction() * config.air_pitch_rad,
        }
    }

    /// SPEC §8: up_des = −gravity_dir_body; error e = a × up_des, where a is the body
    /// axis that should point up ((0, 1, 0), tipped forward by the pitch wish);
    /// wanted angular acceleration kp·e − kd·(ω − wished yaw rate), capped at
    /// `max_angular_accel_rad_s2` for the passenger (SPEC §9.4 g-load); τ = I · that.
    /// A car already spinning fast towards upright the long way round keeps going
    /// (`keep_spin_through`) instead of being stopped and turned back.
    fn attitude_torque(&self, state: &CarState, wish: &AirWish) -> [f32; 3] {
        let [gx, gy, gz] = state.gravity_dir_body;
        let up_wanted = [-gx, -gy, -gz];
        let (sin_p, cos_p) = (
            libm::sinf(wish.nose_down_rad),
            libm::cosf(wish.nose_down_rad),
        );
        let axis_up = [-sin_p, cos_p, 0.0];
        let mut error = cross(axis_up, up_wanted);
        let upside_down = up_wanted[1] < 0.0;
        if upside_down && error[0].abs() < MIN_TILT_ERROR && error[2].abs() < MIN_TILT_ERROR {
            // The cross product vanishes; roll (either way works) at the threshold size,
            // so the torque does not jump as the error crosses it.
            let roll_sign = if up_wanted[2] >= 0.0 { 1.0 } else { -1.0 };
            error = [roll_sign * MIN_TILT_ERROR, 0.0, 0.0];
        }
        let spin = state.angular_velocity_body_rad_s;
        let wished_spin = [0.0, wish.yaw_rate_rad_s, 0.0];
        let (kp, kd) = self.attitude_gains(state);
        let mut accel = [0.0; 3];
        for axis in 0..3 {
            let mut damped_spin = spin[axis] - wished_spin[axis];
            if axis != 1 && self.keep_spin_through(state, error[axis], spin[axis]) {
                // Push on round the way it already turns, without braking the spin.
                error[axis] = -error[axis];
                damped_spin = 0.0;
            }
            accel[axis] = kp * error[axis] - kd * damped_spin;
        }
        let accel = cap_length(accel, self.config.max_angular_accel_rad_s2);
        let mut torque = [0.0; 3];
        for axis in 0..3 {
            torque[axis] = self.inertia_kg_m2[axis] * accel[axis];
        }
        torque
    }

    /// The attitude gains (kp, kd) for the car's tilt: the near-upright pair up to
    /// `tumble_blend_start_deg`, the tumble pair past `tumble_blend_end_deg`, and a
    /// straight-line blend between.
    pub fn attitude_gains(&self, state: &CarState) -> (f32, f32) {
        let config = &self.config;
        // libm keeps the angle identical on every platform (CLAUDE.md rule 2).
        let tilt_deg = libm::acosf(state.up_alignment().clamp(-1.0, 1.0)).to_degrees();
        let band_deg = config.tumble_blend_end_deg - config.tumble_blend_start_deg;
        let share = ((tilt_deg - config.tumble_blend_start_deg) / band_deg).clamp(0.0, 1.0);
        let kp = config.kp + (config.kp_tumble - config.kp) * share;
        let kd = config.kd + (config.kd_tumble - config.kd) * share;
        (kp, kd)
    }

    /// True when the car spins about a roll or pitch axis away from the short way up
    /// (spin and error of opposite signs) so fast that stopping alone would carry it past
    /// the half turn: with s = ω² / (2·α_max), reversing travels θ + 2s but carrying on
    /// only 2π − θ, so carry on when s > π − θ (θ the tilt from upright).
    fn keep_spin_through(&self, state: &CarState, error: f32, spin_rad_s: f32) -> bool {
        if error * spin_rad_s >= 0.0 {
            return false;
        }
        let tilt_rad = libm::acosf(state.up_alignment().clamp(-1.0, 1.0));
        let stopping_rad = spin_rad_s * spin_rad_s / (2.0 * self.config.max_angular_accel_rad_s2);
        stopping_rad > std::f32::consts::PI - tilt_rad
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

/// What the driver asks of the car in the air (`Pd::air_wish`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AirWish {
    /// Wanted yaw rate, positive turning the nose left, in rad/s.
    yaw_rate_rad_s: f32,
    /// Wanted pitch, positive tipping the nose down, in rad.
    nose_down_rad: f32,
}

/// a × b.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `vector` scaled down, if needed, so its length is at most `limit`.
fn cap_length(vector: [f32; 3], limit: f32) -> [f32; 3] {
    let length = libm::sqrtf(vector.iter().map(|value| value * value).sum());
    if length <= limit {
        return vector;
    }
    vector.map(|value| value * limit / length)
}

/// The total upward push (body +Y force) of all nozzles at full throttle, in N.
fn full_lift_n(nozzle_wrenches: &[Wrench; NOZZLE_COUNT]) -> f32 {
    let mut lift_n = 0.0;
    for wrench in nozzle_wrenches {
        lift_n += wrench[1].max(0.0);
    }
    lift_n
}

impl Pd {
    /// The actuator-aware allocation's report on its last solve; None with the legacy
    /// allocation, or when there was nothing to solve.
    pub fn last_allocation(&self) -> Option<&AllocationDiagnostics> {
        self.last_allocation.as_ref()
    }

    /// Plans `wrench` with the actuator predictor and keeps its report. While the
    /// interlock holds, the world would cut any throttles, so it plans none.
    fn allocate_for_actuators(&mut self, wrench: Wrench, actuators: &ActuatorState) {
        let iterations = if actuators.thrusters_locked() {
            self.throttles = [0.0; NOZZLE_COUNT];
            0
        } else {
            let idle = actuators.predict([0.0; NOZZLE_COUNT]);
            let plan = ActuatorPlan::from_idle_prediction(&idle, actuators.controller_gain());
            let scale = actuators.mode_thrust_scale();
            self.allocator
                .solve_actuator_aware(wrench, scale, &plan, &mut self.throttles);
            self.allocator.iterations()
        };
        let prediction = actuators.predict(self.throttles);
        self.last_allocation = Some(AllocationDiagnostics::new(wrench, &prediction, iterations));
    }
}

impl Controller for Pd {
    fn reset(&mut self, _seed: u64) {
        self.throttles = [0.0; NOZZLE_COUNT];
        self.last_allocation = None;
    }

    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        let wrench = self.wanted_wrench_for(&ctx.state, &ctx.input);
        self.allocate(wrench, ctx)
    }
}

impl Pd {
    /// Throttles for `wrench` (all zero when it is zero), the allocation `act` uses.
    fn allocate(&mut self, wrench: Wrench, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        if wrench == [0.0; WRENCH_SIZE] {
            self.throttles = [0.0; NOZZLE_COUNT];
            return self.throttles;
        }
        match (self.allocation_model, &ctx.actuators) {
            (AllocationModel::ActuatorAware, Some(actuators)) => {
                self.allocate_for_actuators(wrench, actuators);
            }
            // Plan with the mode's scale only: dividing by the tank gate would drive every
            // nozzle to full throttle as the tank runs dry (SPEC §7.5 fades it instead).
            _ => self
                .allocator
                .solve(wrench, ctx.state.mode_thrust_scale, &mut self.throttles),
        }
        self.throttles
    }

    /// Throttles that spin the car about body axis `axis` (0 roll, 2 pitch) with angular
    /// acceleration `direction × alpha_rad_s2`, for the stunt director (no force asked).
    pub fn spin_throttles(
        &mut self,
        axis: usize,
        direction: f32,
        alpha_rad_s2: f32,
        ctx: &ControlCtx,
    ) -> [f32; NOZZLE_COUNT] {
        let mut wrench = [0.0; WRENCH_SIZE];
        wrench[3 + axis] = direction * alpha_rad_s2 * self.inertia_kg_m2[axis];
        self.allocate(wrench, ctx)
    }
}
