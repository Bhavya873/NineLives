//! The PD baseline controller (SPEC §8): turn the car upright with attitude torque, brake
//! a fast fall early enough to stop before landing, damp sideways sliding before
//! touchdown, and roll back onto the wheels when lying on the ground (the side boosters
//! pushing too). It decides a wanted force and torque, then the allocation picks throttles.

use nl_sim::car_state::{CarModel, CarState, MAX_HEIGHT_M, WRENCH_SIZE, Wrench};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT};
use nl_sim::forecast::{EDGE_FAR_M, LandingView};
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

/// Centring never counts on less time than this, in s, nor asks for more speed than
/// `MAX_CENTRING_M_S`.
const MIN_CENTRING_TIME_S: f32 = 0.5;
const MAX_CENTRING_M_S: f32 = 3.0;

/// The PD controller with its allocation, for one car.
#[derive(Debug, Clone, PartialEq)]
pub struct Pd {
    config: PdConfig,
    mass_kg: f32,
    inertia_kg_m2: [f32; 3],
    /// Upward push of every nozzle at full throttle in a ×1 mode, along body +Y, in N.
    full_lift_n: f32,
    /// How fast the air leaves a nozzle, in m/s (air used per second = push / this).
    exhaust_speed_m_s: f32,
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
            exhaust_speed_m_s: model.exhaust_speed_m_s,
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
        self.wanted_wrench_with(state, input, None)
    }

    /// As `wanted_wrench_for`, looking ahead with the landing summary: braking from the
    /// real surface below the path, the catch aimed at its normal, sideways damping
    /// stronger near its edge, and (limited air) a long fall left alone.
    pub fn wanted_wrench_with(
        &self,
        state: &CarState,
        input: &InputFrame,
        landing: Option<&LandingView>,
    ) -> Wrench {
        if self.driving_on_wheels(state) {
            return [0.0; WRENCH_SIZE];
        }
        let far = self.touchdown_far(state, landing);
        let mut wish = self.air_wish(state, input);
        if let (Some(view), false) = (landing, far) {
            wish.nose_down_rad += self.edge_pitch_rad(state, view);
        }
        let mut torque = self.attitude_torque(state, &wish, landing, far);
        // Low, tipped over and nearly still: lying on the ground, not falling past it.
        let lying_on_ground = state.height_m < self.config.on_ground_height_m
            && state.up_alignment() < self.config.upright_alignment
            && state.falling_speed_m_s().abs() < self.config.lying_max_fall_speed_m_s;
        let mut force = self.braking_force_body(state, landing);
        if lying_on_ground {
            // SPEC §8: roll torque only, to flip back onto the wheels. Lifting the car
            // over its side edge takes far more torque than turning it in the air.
            torque[0] *= self.config.ground_roll_gain;
            torque[1] = 0.0;
            torque[2] = 0.0;
            force[2] = self.side_push_n(state);
        } else {
            force[2] = self.lateral_damping_n(state, landing, far);
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
    fn attitude_torque(
        &self,
        state: &CarState,
        wish: &AirWish,
        landing: Option<&LandingView>,
        far: bool,
    ) -> [f32; 3] {
        let up_wanted = self.up_wanted(state, landing, far);
        if far && self.holds_attitude(state, &up_wanted) {
            return [0.0; 3];
        }
        let mut error = tilt_error(wish, &up_wanted);
        let accel = self.wanted_accel(state, wish, landing, far, &mut error);
        let accel = cap_length(accel, self.config.max_angular_accel_rad_s2);
        let mut torque = [0.0; 3];
        for axis in 0..3 {
            torque[axis] = self.inertia_kg_m2[axis] * accel[axis];
        }
        torque
    }

    /// The body-frame direction that should point up: against gravity, or, near
    /// touchdown on a tilted surface, the surface normal.
    fn up_wanted(&self, state: &CarState, landing: Option<&LandingView>, far: bool) -> [f32; 3] {
        let [gx, gy, gz] = state.gravity_dir_body;
        let mut up_wanted = [-gx, -gy, -gz];
        if let (Some(view), false) = (landing, far) {
            // Near touchdown on a tilted surface: land flat on it, not flat to the world.
            let min_normal_y = libm::cosf(self.config.normal_aim_max_deg.to_radians());
            let normal = view.normal_body;
            if normal[1] >= min_normal_y && view.gap_m.is_finite() {
                up_wanted = normal;
            }
        }
        up_wanted
    }

    /// The wanted angular acceleration in rad/s^2 (before the g-load cap): kp times the
    /// tilt `error` minus kd times the spin relative to the wished yaw rate. May flip
    /// `error` on an axis the car should keep spinning through.
    fn wanted_accel(
        &self,
        state: &CarState,
        wish: &AirWish,
        landing: Option<&LandingView>,
        far: bool,
        error: &mut [f32; 3],
    ) -> [f32; 3] {
        let spin = state.angular_velocity_body_rad_s;
        let wished_spin = [0.0, wish.yaw_rate_rad_s, 0.0];
        let (kp, kd) = self.attitude_gains(state);
        let air_scale = if far {
            self.low_air_gain_scale(landing)
        } else {
            1.0
        };
        let (kp, kd) = (kp * air_scale, kd * air_scale);
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
        accel
    }

    /// The attitude gains (kp, kd) for the car's tilt: the near-upright pair up to
    /// `tumble_blend_start_deg`, the tumble pair past `tumble_blend_end_deg`, and a
    /// straight-line blend between.
    pub fn attitude_gains(&self, state: &CarState) -> (f32, f32) {
        let config = &self.config;
        // libm keeps the angle identical on every platform (CONTRIBUTING.md rule 2).
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
    fn braking_force_body(&self, state: &CarState, landing: Option<&LandingView>) -> [f32; 3] {
        if !self.should_brake(state, landing) {
            return [0.0; 3];
        }
        let falling_m_s = state.falling_speed_m_s();
        let push_n = self.mass_kg
            * (GRAVITY_M_S2 + self.config.kv * (falling_m_s - self.config.target_descent_m_s));
        [0.0, push_n, 0.0]
    }

    /// Airborne, upright enough, falling faster than `brake_speed_m_s`, and at or below
    /// the height where braking must start (`brake_start_height_m`).
    fn should_brake(&self, state: &CarState, landing: Option<&LandingView>) -> bool {
        // The surface the path comes down on, not only the ground straight below: a
        // parked car or table top ahead is higher, so braking starts sooner.
        // The height ray stops at MAX_HEIGHT_M; with limited air the true gap is used past
        // it, so a long fall is not braked from 60 m up (Arcade keeps the old behaviour).
        let height_m = match landing {
            Some(view) if view.limited_air && state.height_m >= MAX_HEIGHT_M => view.gap_m,
            Some(view) => state.height_m.min(view.gap_m),
            None => state.height_m,
        };
        let margin_m = match landing {
            Some(view) if view.on_obstacle => self.config.obstacle_margin_m,
            _ => 0.0,
        };
        state.wheels_down() == 0
            && state.up_alignment() > self.config.upright_alignment
            && state.falling_speed_m_s() > self.config.brake_speed_m_s
            && height_m < self.brake_start_height_m(state) + margin_m
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
    fn lateral_damping_n(&self, state: &CarState, landing: Option<&LandingView>, far: bool) -> f32 {
        let damp = state.wheels_down() == 0
            && state.up_alignment() > self.config.upright_alignment
            && !far;
        if !damp {
            return 0.0;
        }
        let mut sideways_m_s = state.linear_velocity_body_m_s[2];
        let mut boost = 1.0;
        if let Some(view) = landing {
            let edge_index = if sideways_m_s > 0.0 { 2 } else { 3 };
            boost += self.config.edge_damping_boost * edge_closeness(view, edge_index);
            sideways_m_s -= centring_speed_m_s(view, 2, 3);
        }
        -self.mass_kg * self.config.kv_lateral * boost * sideways_m_s
    }

    /// Landing view in hand, the nose tip (rad, positive nose down) that slows a slide
    /// towards a near edge: nose up when sliding forward, down when backing.
    fn edge_pitch_rad(&self, state: &CarState, view: &LandingView) -> f32 {
        if state.up_alignment() <= self.config.upright_alignment {
            return 0.0;
        }
        let drive_m_s = state.linear_velocity_body_m_s[0];
        if drive_m_s.abs() > MAX_CENTRING_M_S {
            // Too fast to stop on this surface: tipping the nose would only perch the car
            // on its edge. Let it roll through.
            return 0.0;
        }
        let forward_m_s = drive_m_s - centring_speed_m_s(view, 0, 1);
        let tip = (self.config.edge_pitch_rad_per_m_s * forward_m_s).clamp(
            -self.config.edge_pitch_max_rad,
            self.config.edge_pitch_max_rad,
        );
        -tip
    }

    /// Time the final braking takes from the fall speed at touchdown, in s.
    fn braking_time_s(&self, state: &CarState, view: &LandingView) -> f32 {
        let speed_m_s = self.touchdown_fall_speed_m_s(state, view);
        let lift_n = self.full_lift_n * state.mode_thrust_scale * self.config.brake_usable_fraction;
        let deceleration_m_s2 = lift_n / self.mass_kg - GRAVITY_M_S2;
        if deceleration_m_s2 <= 0.0 {
            return f32::INFINITY;
        }
        (speed_m_s - self.config.target_descent_m_s).max(0.0) / deceleration_m_s2
    }

    /// Fall speed when the nominal path reaches the surface, in m/s.
    fn touchdown_fall_speed_m_s(&self, state: &CarState, view: &LandingView) -> f32 {
        (state.falling_speed_m_s() + GRAVITY_M_S2 * view.time_s).max(0.0)
    }

    /// Limited air only: touchdown is still farther than the braking time plus the catch
    /// window, so attitude holds and sideways damping can wait. False in unlimited air, on
    /// the ground and where nobody filled the landing view in.
    fn touchdown_far(&self, state: &CarState, landing: Option<&LandingView>) -> bool {
        let Some(view) = landing else {
            return false;
        };
        if !view.limited_air {
            return false;
        }
        if !view.time_s.is_finite() {
            return true;
        }
        view.time_s > self.braking_time_s(state, view) + self.config.catch_window_s
    }

    /// Far from touchdown and nearly level and quiet: leave the attitude alone.
    fn holds_attitude(&self, state: &CarState, up_wanted: &[f32; 3]) -> bool {
        let tilt_deg = libm::acosf(up_wanted[1].clamp(-1.0, 1.0)).to_degrees();
        tilt_deg < self.config.hold_deadband_deg
            && state.spin_rad_s() < self.config.hold_max_spin_rad_s
    }

    /// The share of the attitude gains kept when the tank is low: full above
    /// `low_air_floor_kg`, sliding down to `low_air_gain_share` with an empty tank.
    fn low_air_gain_scale(&self, landing: Option<&LandingView>) -> f32 {
        let Some(view) = landing else {
            return 1.0;
        };
        let share = self.config.low_air_gain_share;
        let filled = (view.air_kg / self.config.low_air_floor_kg).clamp(0.0, 1.0);
        share + (1.0 - share) * filled
    }

    /// The air the landing needs from here, in kg: the final braking from the fall speed
    /// at touchdown (push × time ÷ exhaust speed, with the weight it also carries) times
    /// `reserve_safety`, plus the last catch. 0 where there is no landing view.
    pub fn landing_reserve_kg(&self, state: &CarState, landing: Option<&LandingView>) -> f32 {
        let Some(view) = landing else {
            return 0.0;
        };
        let speed_m_s = self.touchdown_fall_speed_m_s(state, view);
        let time_s = self.braking_time_s(state, view);
        if !time_s.is_finite() {
            return f32::INFINITY;
        }
        let momentum = self.mass_kg * (speed_m_s + GRAVITY_M_S2 * time_s);
        self.config.reserve_safety * momentum / self.exhaust_speed_m_s
            + self.config.reserve_catch_air_kg
    }

    /// Lying on its side: push along body Z towards world up with `ground_side_push` ×
    /// the car's weight, scaled by how far that side axis points up (world up in the body
    /// frame is −gravity_dir_body).
    fn side_push_n(&self, state: &CarState) -> f32 {
        let up_along_side = -state.gravity_dir_body[2];
        self.config.ground_side_push * self.mass_kg * GRAVITY_M_S2 * up_along_side
    }
}

/// 0 far from the edge in direction `edge_index` (`LandingView::edge_m` order), 1 at it.
fn edge_closeness(view: &LandingView, edge_index: usize) -> f32 {
    (1.0 - view.edge_m[edge_index] / EDGE_FAR_M).clamp(0.0, 1.0)
}

/// The speed along an axis that would bring the touchdown point to the middle of the
/// surface by touchdown, in m/s: the offset to the middle (half the difference of the two
/// edge distances, `plus_edge` and `minus_edge` indices into `LandingView::edge_m`) over
/// the time left, at most `MAX_CENTRING_M_S`.
fn centring_speed_m_s(view: &LandingView, plus_edge: usize, minus_edge: usize) -> f32 {
    if !view.time_s.is_finite() {
        return 0.0;
    }
    let to_middle_m = 0.5 * (view.edge_m[plus_edge] - view.edge_m[minus_edge]);
    (to_middle_m / view.time_s.max(MIN_CENTRING_TIME_S)).clamp(-MAX_CENTRING_M_S, MAX_CENTRING_M_S)
}

/// What the driver asks of the car in the air (`Pd::air_wish`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AirWish {
    /// Wanted yaw rate, positive turning the nose left, in rad/s.
    yaw_rate_rad_s: f32,
    /// Wanted pitch, positive tipping the nose down, in rad.
    nose_down_rad: f32,
}

/// The tilt error `axis_up x up_wanted`, where `axis_up` is the body axis that should
/// point up (tipped forward by the pitch wish). Upside down, where the cross product
/// vanishes, it rolls at `MIN_TILT_ERROR` instead.
fn tilt_error(wish: &AirWish, up_wanted: &[f32; 3]) -> [f32; 3] {
    let (sin_p, cos_p) = (
        libm::sinf(wish.nose_down_rad),
        libm::cosf(wish.nose_down_rad),
    );
    let axis_up = [-sin_p, cos_p, 0.0];
    let mut error = cross(axis_up, *up_wanted);
    let upside_down = up_wanted[1] < 0.0;
    if upside_down && error[0].abs() < MIN_TILT_ERROR && error[2].abs() < MIN_TILT_ERROR {
        // The cross product vanishes; roll (either way works) at the threshold size,
        // so the torque does not jump as the error crosses it.
        let roll_sign = if up_wanted[2] >= 0.0 { 1.0 } else { -1.0 };
        error = [roll_sign * MIN_TILT_ERROR, 0.0, 0.0];
    }
    error
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
        let wrench = self.wanted_wrench_with(&ctx.state, &ctx.input, ctx.landing.as_ref());
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
