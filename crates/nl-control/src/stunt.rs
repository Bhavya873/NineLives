//! The stunt planner (plan 2026-10-27 G4, `docs/contracts/stunt-states.md`,
//! `docs/research/stunt-planning.md` §2).
//!
//! `StuntPlanner` is the state machine and the authority: it decides whether a requested
//! stunt may start (four rules), tracks the rotation about the stunt axis, replans every
//! control tick, and aborts to PD's Catch if the landing window shrinks. It does not move
//! any nozzle itself: each tick it returns a [`StuntCommand`] (spin about an axis, or
//! nothing) and who owns the nozzles; the executor turns the command into throttles.
//!
//! Phases: `Idle → Requested → Authorized → SpinUp → Coast → Brake → Catch → Settled`,
//! with `Aborted` reachable from SpinUp, Coast and Brake. `Authorized` lasts a single
//! tick, so `tick` reports the phase after it, `SpinUp`.

use std::f32::consts::TAU;

use nl_sim::car_state::CarState;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::forecast::{ForecastSettings, LandingForecast};
use nl_sim::world::World;

use crate::config::StuntConfig;
use crate::{ControlCtx, Controller, Pd};

/// Valve lag the switching curve leads by, in s (research §2.2).
const VALVE_LAG_S: f32 = 0.03;
/// A tank gate below this is "not full" for the start envelope (rule 4).
const TANK_FULL_GATE: f32 = 0.999;
/// Below this, thrust is too weak to plan with.
const MIN_ALPHA_RAD_S2: f32 = 0.5;
/// Settled needs the car this close to upright, in degrees, and this slow.
const SETTLED_MAX_TILT_DEG: f32 = 15.0;

/// Body axis indices into `CarState::angular_velocity_body_rad_s`.
const ROLL_AXIS: usize = 0;
const PITCH_AXIS: usize = 2;

/// Which stunt the player asked for. The numbers are the wasm `StuntCode` HUD values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StuntKind {
    BarrelLeft = 1,
    BarrelRight = 2,
    FrontFlip = 3,
    BackFlip = 4,
}

impl StuntKind {
    /// The body axis the car turns about: roll (X) for barrels, pitch (Z) for flips.
    pub fn axis(self) -> usize {
        match self {
            StuntKind::BarrelLeft | StuntKind::BarrelRight => ROLL_AXIS,
            StuntKind::FrontFlip | StuntKind::BackFlip => PITCH_AXIS,
        }
    }

    /// The sign of the body rate about `axis()` that makes progress. Rotation about +Z
    /// takes the nose (+X) towards up (+Y), so a back flip is positive and a front flip
    /// (nose down) negative; rotation about +X takes up (+Y) towards right (+Z), so a
    /// barrel roll right (right side down) is positive.
    pub fn sign(self) -> f32 {
        match self {
            StuntKind::BarrelRight | StuntKind::BackFlip => 1.0,
            StuntKind::BarrelLeft | StuntKind::FrontFlip => -1.0,
        }
    }

    pub fn is_flip(self) -> bool {
        self.axis() == PITCH_AXIS
    }

    /// The HUD `StuntCode` value.
    pub fn code(self) -> u8 {
        self as u8
    }

    /// The stunt a `StuntCode` value names, `None` for 0 or an unknown number.
    pub fn from_code(code: u8) -> Option<StuntKind> {
        [
            StuntKind::BarrelLeft,
            StuntKind::BarrelRight,
            StuntKind::FrontFlip,
            StuntKind::BackFlip,
        ]
        .into_iter()
        .find(|kind| kind.code() == code)
    }
}

/// Where a stunt is. The numbers are the wasm `StuntPhase` HUD values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StuntPhase {
    Idle = 0,
    Requested = 1,
    Authorized = 2,
    SpinUp = 3,
    Coast = 4,
    Brake = 5,
    Catch = 6,
    Settled = 7,
    Aborted = 8,
}

impl StuntPhase {
    pub fn code(self) -> u8 {
        self as u8
    }

    /// Exactly one owner per phase (docs/contracts/stunt-states.md). `Requested` and
    /// `Idle` belong to PD too: no stunt is flying.
    pub fn owner(self) -> NozzleOwner {
        match self {
            StuntPhase::SpinUp | StuntPhase::Coast | StuntPhase::Brake => {
                NozzleOwner::StuntDirector
            }
            _ => NozzleOwner::Pd,
        }
    }
}

/// Who writes the nozzles this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NozzleOwner {
    StuntDirector,
    Pd,
}

/// Which mode's rates the plan uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StuntMode {
    Realistic,
    Arcade,
}

/// Why a stunt was refused or aborted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StuntReason {
    /// No landing is forecast.
    NoLanding,
    /// Rule 1: the landing surface is tilted more than the limit.
    LandingTilted,
    /// Rule 1: the flat, obstacle-free surface is shorter than two car lengths.
    LandingBlocked,
    /// Rule 2: not enough air time for spin, brake, catch and margin.
    NotEnoughTime,
    /// Rule 3: not enough air in the tank.
    NotEnoughAir,
    /// Rule 4: the car is not airborne.
    NotAirborne,
    /// Rule 4: tilted more than the start envelope allows.
    TiltedTooFar,
    /// Rule 4: already spinning faster than the start envelope allows.
    SpinningTooFast,
    /// Rule 4: the tank gate is not 1.
    TankNotFull,
    /// Rule 4: the mode's thrust authority is not known (zero or not a number).
    ModeUnknown,
    /// The request was not authorized within `request_timeout_s`.
    RequestExpired,
    /// Aborted: thrust ran out or the car touched the ground early.
    ControlLost,
}

/// What the planner reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StuntEvent {
    /// An authorized stunt began SpinUp (wasm `StuntStart`, 11).
    Started { kind: StuntKind },
    /// The stunt reached Settled with a full rotation (`StuntLanded`, 12).
    Landed { kind: StuntKind, rotation_rad: f32 },
    /// The planner said no (`StuntRefused`, 13).
    Refused {
        kind: StuntKind,
        reason: StuntReason,
    },
    /// The planner stopped the stunt mid-air and handed over to PD (`StuntAborted`, 14).
    Aborted {
        kind: StuntKind,
        reason: StuntReason,
        rotation_rad: f32,
    },
}

/// What the car and the world look like to the planner this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlannerInputs {
    pub time_s: f32,
    /// The time this tick covers, used to integrate the rotation, in s.
    pub dt_s: f32,
    pub state: CarState,
    pub air_kg: f32,
    pub mode: StuntMode,
    /// Arcade-style tank that never runs out: only the time rules apply.
    pub unlimited_air: bool,
    /// None if nothing is forecast to be hit.
    pub forecast: Option<LandingForecast>,
}

impl PlannerInputs {
    /// What `world` shows now, with the forecast made under `config`'s sample spreads.
    pub fn from_world(world: &World, config: &StuntConfig, dt_s: f32) -> PlannerInputs {
        let mode = world.mode();
        PlannerInputs {
            time_s: world.time_s() as f32,
            dt_s,
            state: world.car_state(),
            air_kg: world.air_kg(),
            mode: if mode.unlimited_air {
                StuntMode::Arcade
            } else {
                StuntMode::Realistic
            },
            unlimited_air: mode.unlimited_air,
            forecast: world.forecast_landing_with(&forecast_settings(config)),
        }
    }
}

/// The forecast settings that follow from the `[stunt]` table.
pub fn forecast_settings(config: &StuntConfig) -> ForecastSettings {
    ForecastSettings {
        speed_spread: config.forecast_speed_spread,
        pitch_spread_deg: config.forecast_pitch_spread_deg,
        flat_tilt_deg: config.landing_max_normal_tilt_deg,
        ..ForecastSettings::default()
    }
}

/// The numbers a stunt is planned with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StuntPlan {
    /// The spin the car coasts at, rad/s (capped so spin-up and brake fit in one turn).
    pub coast_rate_rad_s: f32,
    /// Angular acceleration about the stunt axis now, rad/s².
    pub alpha_rad_s2: f32,
    /// Spin-up, coast and brake times from rest, s.
    pub spin_up_s: f32,
    pub coast_s: f32,
    pub brake_s: f32,
    /// The air time the whole stunt needs: spin-up + coast + brake + catch + margin, s.
    pub required_s: f32,
    /// The air the stunt needs (rule 3), kg; 0 with unlimited air.
    pub air_needed_kg: f32,
}

/// A refusal and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StuntRefusal {
    pub reason: StuntReason,
}

/// Spin about the stunt axis for this tick; nothing is asked of the nozzles while coasting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StuntCommand {
    /// Body axis index: 0 roll (X), 2 pitch (Z).
    pub axis: usize,
    /// Direction of full torque about that body axis: +1 or -1.
    pub direction: f32,
}

/// What `tick` decided.
#[derive(Debug, Clone, PartialEq)]
pub struct StuntTick {
    pub phase: StuntPhase,
    pub owner: NozzleOwner,
    /// `Some` only while the StuntDirector owns the nozzles and wants torque.
    pub command: Option<StuntCommand>,
    pub events: Vec<StuntEvent>,
}

fn alpha_for(config: &StuntConfig, kind: StuntKind, state: &CarState) -> f32 {
    let base = if kind.is_flip() {
        config.pitch_alpha_rad_s2
    } else {
        config.roll_alpha_rad_s2
    };
    base * state.mode_thrust_scale * state.tank_gate
}

fn coast_rate_for(config: &StuntConfig, kind: StuntKind, mode: StuntMode, alpha: f32) -> f32 {
    let configured = match (kind.is_flip(), mode) {
        (true, StuntMode::Realistic) => config.flip_coast_rate_realistic_rad_s,
        (false, StuntMode::Realistic) => config.barrel_coast_rate_realistic_rad_s,
        (true, StuntMode::Arcade) => config.flip_coast_rate_arcade_rad_s,
        (false, StuntMode::Arcade) => config.barrel_coast_rate_arcade_rad_s,
    };
    // Spin-up and brake together must fit in one turn: w² <= 2·pi·alpha + wh²/2.
    let fits = (TAU * alpha + 0.5 * config.handover_rate_rad_s.powi(2)).sqrt();
    configured.min(fits)
}

/// Angle of spin-up from rest (or from `from` rad/s) to `to` rad/s at `alpha`.
fn ramp_angle(from: f32, to: f32, alpha: f32) -> f32 {
    ((to * to - from * from) / (2.0 * alpha)).max(0.0)
}

/// Rule 1: the landing surface. `Err` carries which part failed.
fn check_landing(config: &StuntConfig, landing: &LandingForecast) -> Result<(), StuntReason> {
    if landing.normal_tilt_deg > config.landing_max_normal_tilt_deg {
        return Err(StuntReason::LandingTilted);
    }
    if landing.usable_length_m < config.landing_min_length_car_lengths * config.car_length_m {
        return Err(StuntReason::LandingBlocked);
    }
    Ok(())
}

/// Rule 4: the start envelope.
fn check_start(config: &StuntConfig, inputs: &PlannerInputs) -> Result<(), StuntReason> {
    let state = &inputs.state;
    if !(state.mode_thrust_scale.is_finite() && state.mode_thrust_scale > 0.0) {
        return Err(StuntReason::ModeUnknown);
    }
    if state.wheels_down() > 0 {
        return Err(StuntReason::NotAirborne);
    }
    if state.up_alignment().clamp(-1.0, 1.0).acos().to_degrees() > config.start_max_tilt_deg {
        return Err(StuntReason::TiltedTooFar);
    }
    if state.spin_rad_s() > config.start_max_spin_rad_s {
        return Err(StuntReason::SpinningTooFast);
    }
    if state.tank_gate < TANK_FULL_GATE {
        return Err(StuntReason::TankNotFull);
    }
    Ok(())
}

/// The planner's four authorization rules (research §2.3, plan G4). `Ok` carries the plan,
/// `Err` the first rule that failed, in the order: start envelope (4), landing surface (1),
/// time (2), air (3).
pub fn authorize(
    config: &StuntConfig,
    kind: StuntKind,
    inputs: &PlannerInputs,
) -> Result<StuntPlan, StuntRefusal> {
    let refuse = |reason| Err(StuntRefusal { reason });
    if let Err(reason) = check_start(config, inputs) {
        return refuse(reason);
    }
    let Some(landing) = inputs.forecast else {
        return refuse(StuntReason::NoLanding);
    };
    if let Err(reason) = check_landing(config, &landing) {
        return refuse(reason);
    }
    let alpha = alpha_for(config, kind, &inputs.state);
    let omega = coast_rate_for(config, kind, inputs.mode, alpha);
    let handover = config.handover_rate_rad_s;
    let spin_up_s = omega / alpha;
    let brake_s = (omega - handover).max(0.0) / alpha;
    let turned = ramp_angle(0.0, omega, alpha) + ramp_angle(handover, omega, alpha);
    let coast_s = (TAU - turned).max(0.0) / omega;
    let required_s = spin_up_s + coast_s + brake_s + config.catch_time_s + config.settle_margin_s;
    // Rule 2.
    if landing.time_s - landing.time_uncertainty_s < required_s {
        return refuse(StuntReason::NotEnoughTime);
    }
    // Rule 3.
    let air_rate = if kind.is_flip() {
        config.pitch_air_kg_s
    } else {
        config.roll_air_kg_s
    };
    let air_needed_kg = if inputs.unlimited_air {
        0.0
    } else {
        config.air_margin * (air_rate * (spin_up_s + brake_s) + config.catch_air_kg)
            + config.landing_reserve_kg
    };
    if !inputs.unlimited_air && inputs.air_kg < air_needed_kg {
        return refuse(StuntReason::NotEnoughAir);
    }
    Ok(StuntPlan {
        coast_rate_rad_s: omega,
        alpha_rad_s2: alpha,
        spin_up_s,
        coast_s,
        brake_s,
        required_s,
        air_needed_kg,
    })
}

/// One stunt's state machine. Make one per car, call `request` when the player presses a
/// stunt key and `tick` every control tick.
#[derive(Debug, Clone)]
pub struct StuntPlanner {
    config: StuntConfig,
    phase: StuntPhase,
    kind: StuntKind,
    requested_at_s: f32,
    settled_at_s: f32,
    /// Integrated rotation about the stunt axis, in the direction of the stunt, rad.
    rotation_rad: f32,
    coast_rate_rad_s: f32,
}

impl StuntPlanner {
    pub fn new(config: StuntConfig) -> StuntPlanner {
        StuntPlanner {
            config,
            phase: StuntPhase::Idle,
            kind: StuntKind::FrontFlip,
            requested_at_s: 0.0,
            settled_at_s: 0.0,
            rotation_rad: 0.0,
            coast_rate_rad_s: 0.0,
        }
    }

    pub fn config(&self) -> &StuntConfig {
        &self.config
    }

    pub fn phase(&self) -> StuntPhase {
        self.phase
    }

    pub fn kind(&self) -> StuntKind {
        self.kind
    }

    /// Rotation about the stunt axis so far, in the stunt's direction, rad.
    pub fn rotation_rad(&self) -> f32 {
        self.rotation_rad
    }

    /// True once the integrated rotation reached 2π minus the slack. Being upright is not
    /// enough.
    pub fn rotation_complete(&self) -> bool {
        self.rotation_rad >= TAU - self.config.complete_rotation_slack_rad
    }

    /// True while the Hybrid gate must stay on PD: from Authorized until
    /// `hybrid_hold_after_settled_s` after Settled.
    pub fn hybrid_hold_active(&self) -> bool {
        !matches!(self.phase, StuntPhase::Idle | StuntPhase::Requested)
    }

    /// Queues a stunt. Ignored (returns false) while another stunt is queued or flying.
    pub fn request(&mut self, kind: StuntKind, now_s: f32) -> bool {
        if self.phase != StuntPhase::Idle {
            return false;
        }
        self.phase = StuntPhase::Requested;
        self.kind = kind;
        self.requested_at_s = now_s;
        true
    }

    /// Drops any stunt (respawn or reset).
    pub fn reset(&mut self) {
        self.phase = StuntPhase::Idle;
        self.rotation_rad = 0.0;
    }

    /// `tick` for a world: builds the inputs, with `dt_s` the time the tick covers.
    pub fn tick_world(&mut self, world: &World, dt_s: f32) -> StuntTick {
        let inputs = PlannerInputs::from_world(world, &self.config, dt_s);
        self.tick(&inputs)
    }

    /// Advances the machine by one control tick: tracks the rotation, replans against the
    /// fresh forecast, and says what the nozzles should do and who owns them.
    pub fn tick(&mut self, inputs: &PlannerInputs) -> StuntTick {
        let mut events = Vec::new();
        match self.phase {
            StuntPhase::Idle => {}
            StuntPhase::Requested => self.tick_requested(inputs, &mut events),
            StuntPhase::Authorized => self.phase = StuntPhase::SpinUp,
            StuntPhase::SpinUp | StuntPhase::Coast | StuntPhase::Brake => {
                self.track_rotation(inputs);
                self.tick_flying(inputs, &mut events);
            }
            StuntPhase::Catch | StuntPhase::Aborted => {
                self.track_rotation(inputs);
                self.tick_recovery(inputs, &mut events);
            }
            StuntPhase::Settled => {
                if inputs.time_s - self.settled_at_s >= self.config.hybrid_hold_after_settled_s {
                    self.phase = StuntPhase::Idle;
                    self.rotation_rad = 0.0;
                }
            }
        }
        StuntTick {
            phase: self.phase,
            owner: self.phase.owner(),
            command: self.command(inputs),
            events,
        }
    }

    fn tick_requested(&mut self, inputs: &PlannerInputs, events: &mut Vec<StuntEvent>) {
        let kind = self.kind;
        match authorize(&self.config, kind, inputs) {
            Ok(plan) => {
                self.coast_rate_rad_s = plan.coast_rate_rad_s;
                self.rotation_rad = 0.0;
                // Authorized lasts this one tick, then SpinUp: the nozzles are fired from
                // the next tick on, so report SpinUp now.
                self.phase = StuntPhase::SpinUp;
                events.push(StuntEvent::Started { kind });
            }
            // Not in the air yet: keep the request queued until it expires.
            Err(StuntRefusal {
                reason: StuntReason::NotAirborne,
            }) if inputs.time_s - self.requested_at_s < self.config.request_timeout_s => {}
            Err(StuntRefusal {
                reason: StuntReason::NotAirborne,
            }) => {
                self.phase = StuntPhase::Idle;
                events.push(StuntEvent::Refused {
                    kind,
                    reason: StuntReason::RequestExpired,
                });
            }
            Err(StuntRefusal { reason }) => {
                self.phase = StuntPhase::Idle;
                events.push(StuntEvent::Refused { kind, reason });
            }
        }
    }

    /// Adds this tick's turn about the stunt axis: body rate × dt, in the stunt's direction.
    fn track_rotation(&mut self, inputs: &PlannerInputs) {
        let rate = self.kind.sign() * inputs.state.angular_velocity_body_rad_s[self.kind.axis()];
        self.rotation_rad += rate * inputs.dt_s;
    }

    /// Spin rate in the stunt's direction, rad/s.
    fn progress_rate(&self, inputs: &PlannerInputs) -> f32 {
        self.kind.sign() * inputs.state.angular_velocity_body_rad_s[self.kind.axis()]
    }

    fn tick_flying(&mut self, inputs: &PlannerInputs, events: &mut Vec<StuntEvent>) {
        let cfg = self.config;
        let rate = self.progress_rate(inputs);
        let alpha = alpha_for(&cfg, self.kind, &inputs.state);

        // Replan against the fresh forecast; abort to Catch if the window shrank.
        if let Some(reason) = self.window_problem(inputs, alpha, rate) {
            self.abort(reason, events);
            return;
        }

        // Phase changes. Brake as soon as the remaining angle reaches the switching curve.
        let handover = cfg.handover_rate_rad_s;
        let remaining = TAU - self.rotation_rad;
        let curve = ramp_angle(handover, rate.max(handover), alpha) + rate.max(0.0) * VALVE_LAG_S;
        match self.phase {
            StuntPhase::SpinUp | StuntPhase::Coast if remaining <= curve => {
                self.phase = StuntPhase::Brake;
            }
            StuntPhase::SpinUp if rate >= self.coast_rate_rad_s => self.phase = StuntPhase::Coast,
            _ => {}
        }
        if self.phase == StuntPhase::Brake && rate <= handover {
            self.phase = StuntPhase::Catch;
        }
    }

    /// Why the stunt can no longer be completed, if it cannot (replanned every tick).
    fn window_problem(&self, inputs: &PlannerInputs, alpha: f32, rate: f32) -> Option<StuntReason> {
        let cfg = &self.config;
        if inputs.state.wheels_down() > 0 || alpha < MIN_ALPHA_RAD_S2 {
            return Some(StuntReason::ControlLost);
        }
        let Some(landing) = inputs.forecast else {
            return Some(StuntReason::NoLanding);
        };
        if let Err(reason) = check_landing(cfg, &landing) {
            return Some(reason);
        }
        let handover = cfg.handover_rate_rad_s;
        let omega = self.coast_rate_rad_s;
        let brake_s = (rate.max(handover) - handover) / alpha;
        let remaining_s = match self.phase {
            // Past the point of braking the rotation is what it is: only the recovery must fit.
            StuntPhase::Brake => brake_s,
            StuntPhase::Coast => {
                let to_turn =
                    (TAU - self.rotation_rad - ramp_angle(handover, rate, alpha)).max(0.0);
                to_turn / rate.max(0.1) + brake_s
            }
            _ => {
                let up_s = (omega - rate).max(0.0) / alpha;
                let turned = ramp_angle(rate, omega, alpha) + ramp_angle(handover, omega, alpha);
                let coast_s = (TAU - self.rotation_rad - turned).max(0.0) / omega;
                up_s + coast_s + (omega - handover).max(0.0) / alpha
            }
        };
        let window_s = landing.time_s - landing.time_uncertainty_s;
        if window_s < remaining_s + cfg.catch_time_s + cfg.settle_margin_s {
            return Some(StuntReason::NotEnoughTime);
        }
        if !inputs.unlimited_air && self.phase != StuntPhase::Brake {
            let air_rate = if self.kind.is_flip() {
                cfg.pitch_air_kg_s
            } else {
                cfg.roll_air_kg_s
            };
            let firing_s = (omega - rate).max(0.0) / alpha + (omega - handover).max(0.0) / alpha;
            let needed =
                cfg.air_margin * (air_rate * firing_s + cfg.catch_air_kg) + cfg.landing_reserve_kg;
            if inputs.air_kg < needed {
                return Some(StuntReason::NotEnoughAir);
            }
        }
        None
    }

    fn abort(&mut self, reason: StuntReason, events: &mut Vec<StuntEvent>) {
        self.phase = StuntPhase::Aborted;
        events.push(StuntEvent::Aborted {
            kind: self.kind,
            reason,
            rotation_rad: self.rotation_rad,
        });
    }

    /// Catch and Aborted: PD flies. Settled once the car is on its wheels and quiet.
    fn tick_recovery(&mut self, inputs: &PlannerInputs, events: &mut Vec<StuntEvent>) {
        let state = &inputs.state;
        let tilt_deg = state.up_alignment().clamp(-1.0, 1.0).acos().to_degrees();
        let quiet = state.on_wheels()
            && tilt_deg <= SETTLED_MAX_TILT_DEG
            && state.spin_rad_s() <= self.config.start_max_spin_rad_s;
        if !quiet {
            return;
        }
        let aborted = self.phase == StuntPhase::Aborted;
        self.phase = StuntPhase::Settled;
        self.settled_at_s = inputs.time_s;
        if !aborted && self.rotation_complete() {
            events.push(StuntEvent::Landed {
                kind: self.kind,
                rotation_rad: self.rotation_rad,
            });
        } else if !aborted {
            // Upright, but the rotation was never finished: that is not a stunt.
            events.push(StuntEvent::Aborted {
                kind: self.kind,
                reason: StuntReason::NotEnoughTime,
                rotation_rad: self.rotation_rad,
            });
        }
    }

    fn command(&self, inputs: &PlannerInputs) -> Option<StuntCommand> {
        let axis = self.kind.axis();
        let direction = match self.phase {
            StuntPhase::SpinUp => self.kind.sign(),
            // Counter-torque against the spin as it is, so a brake never adds spin.
            StuntPhase::Brake => -inputs.state.angular_velocity_body_rad_s[axis].signum(),
            _ => return None,
        };
        Some(StuntCommand { axis, direction })
    }
}

/// What the director decided for one control tick.
#[derive(Debug, Clone, PartialEq)]
pub struct StuntStep {
    /// What the planner reported (phase, owner, events).
    pub tick: StuntTick,
    /// The throttles to use this tick when a stunt owns the nozzles (spin, brake, coast)
    /// or PD must hold them (catch, settle); `None` when the active controller flies.
    pub throttles: Option<[f32; NOZZLE_COUNT]>,
}

/// The planner and its executor (plan G4): the planner decides, the director turns the
/// command into throttles through PD's allocation, and PD flies the catch, so exactly one
/// of them writes the nozzles each tick. With no stunt requested it does nothing, and the
/// active controller flies as before.
#[derive(Debug, Clone)]
pub struct StuntDirector {
    planner: StuntPlanner,
    pd: Pd,
}

impl StuntDirector {
    /// A director with `config`'s planner; `pd` is the catch controller and the allocator
    /// the spin torque goes through.
    pub fn new(config: StuntConfig, pd: Pd) -> StuntDirector {
        StuntDirector {
            planner: StuntPlanner::new(config),
            pd,
        }
    }

    /// The planner, for its phase, rotation and Hybrid hold.
    pub fn planner(&self) -> &StuntPlanner {
        &self.planner
    }

    /// Queues a request for `kind` at world time `now_s`; false if one is already queued
    /// or active.
    pub fn request(&mut self, kind: StuntKind, now_s: f32) -> bool {
        self.planner.request(kind, now_s)
    }

    /// Back to Idle with PD reset, on a respawn or a controller change.
    pub fn reset(&mut self) {
        self.planner.reset();
        self.pd.reset(0);
    }

    /// One control tick covering `dt_s`: replans against `world` and says whether the
    /// director has the nozzles.
    pub fn step(&mut self, world: &World, ctx: &ControlCtx, dt_s: f32) -> StuntStep {
        let tick = self.planner.tick_world(world, dt_s);
        let throttles = match (tick.owner, tick.command) {
            (NozzleOwner::StuntDirector, Some(command)) => {
                let config = self.planner.config();
                let alpha = if self.planner.kind().is_flip() {
                    config.pitch_alpha_rad_s2
                } else {
                    config.roll_alpha_rad_s2
                };
                Some(
                    self.pd
                        .spin_throttles(command.axis, command.direction, alpha, ctx),
                )
            }
            (NozzleOwner::StuntDirector, None) => Some([0.0; NOZZLE_COUNT]),
            (NozzleOwner::Pd, _) if self.planner.hybrid_hold_active() => Some(self.pd.act(ctx)),
            _ => None,
        };
        StuntStep { tick, throttles }
    }
}
