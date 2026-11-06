//! The stunt planner (plan 2026-10-27 G4, `docs/contracts/stunt-states.md`,
//! `docs/research/stunt-planning.md` §2).
//!
//! `StuntPlanner` is the state machine and the authority: it decides whether a requested
//! stunt may start (four rules), tracks the rotation about the stunt axis, replans every
//! control tick, and aborts to PD's Catch if the landing window shrinks. It does not move
//! any nozzle itself: each tick it returns a [`StuntCommand`] (spin about an axis, or
//! nothing) and who owns the nozzles; the executor turns the command into throttles.
//!
//! A jump may hold up to [`MAX_STUNTS`] stunts: the same stunt repeated is one long coast
//! (a double flip is one spin-up, two turns, one brake), a different stunt next brakes to a
//! stop and spins up again. Every added stunt re-runs the rules with the extra time and air.
//!
//! Phases: `Idle → Requested → Authorized → SpinUp → Coast → Brake → Catch → Settled`,
//! with `Aborted` reachable from SpinUp, Coast and Brake. `Authorized` lasts a single
//! tick, so `tick` reports the phase after it, `SpinUp`.

use std::f32::consts::TAU;

use nl_sim::car_state::CarState;
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT};
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

/// Most stunts in one jump (rotations counted one each).
pub const MAX_STUNTS: usize = 3;
/// A brake between two spins in one jump ends at this rate, rad/s (not the PD hand-over).
const STOP_RATE_RAD_S: f32 = 0.3;

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

/// The spin the car coasts at. One turn uses the configured rate; several turns may go as
/// fast as the fastest configured rate, which shortens a long coast, as long as spin-up and
/// brake still fit in the turns (w² <= turns·2·pi·alpha + end²/2).
fn coast_rate_for(
    config: &StuntConfig,
    kind: StuntKind,
    mode: StuntMode,
    alpha: f32,
    turns: u8,
    end_rate: f32,
) -> f32 {
    let configured = match (kind.is_flip(), mode) {
        (true, StuntMode::Realistic) => config.flip_coast_rate_realistic_rad_s,
        (false, StuntMode::Realistic) => config.barrel_coast_rate_realistic_rad_s,
        (true, StuntMode::Arcade) => config.flip_coast_rate_arcade_rad_s,
        (false, StuntMode::Arcade) => config.barrel_coast_rate_arcade_rad_s,
    };
    let fits = (f32::from(turns.max(1)) * TAU * alpha + 0.5 * end_rate.powi(2)).sqrt();
    if turns > 1 {
        let fastest = config
            .flip_coast_rate_realistic_rad_s
            .max(config.barrel_coast_rate_realistic_rad_s)
            .max(config.flip_coast_rate_arcade_rad_s)
            .max(config.barrel_coast_rate_arcade_rad_s);
        return configured.max(fastest).min(fits);
    }
    configured.min(fits)
}

/// A run of one stunt kind flown as a single spin of `turns` full rotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    kind: StuntKind,
    turns: u8,
}

/// Merges neighbours of the same kind: a double flip is one segment of two turns.
fn segments_of(kinds: &[StuntKind]) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for &kind in kinds {
        match segments.last_mut() {
            Some(last) if last.kind == kind => last.turns += 1,
            _ => segments.push(Segment { kind, turns: 1 }),
        }
    }
    segments
}

/// The rate a segment's brake ends at: the PD hand-over for the last one, a stop otherwise.
fn end_rate_for(config: &StuntConfig, last: bool) -> f32 {
    if last {
        config.handover_rate_rad_s
    } else {
        STOP_RATE_RAD_S
    }
}

fn air_rate_for(config: &StuntConfig, kind: StuntKind) -> f32 {
    if kind.is_flip() {
        config.pitch_air_kg_s
    } else {
        config.roll_air_kg_s
    }
}

/// Time (s) and firing time (s) of a segment's remaining spin-up, coast and brake.
struct SegmentTimes {
    up_s: f32,
    coast_s: f32,
    brake_s: f32,
}

/// The remaining times of a segment that is at `rate` with `rotation` turned so far,
/// before it has started braking (spin-up and coast), aiming for `omega`.
fn times_to_go(
    segment: Segment,
    omega: f32,
    alpha: f32,
    end_rate: f32,
    rate: f32,
    rotation: f32,
) -> SegmentTimes {
    let target = f32::from(segment.turns) * TAU;
    let up_s = (omega - rate).max(0.0) / alpha;
    let turned = ramp_angle(rate, omega, alpha) + ramp_angle(end_rate, omega, alpha);
    let coast_s = (target - rotation - turned).max(0.0) / omega;
    SegmentTimes {
        up_s,
        coast_s,
        brake_s: (omega - end_rate).max(0.0) / alpha,
    }
}

/// What the rest of a jump's stunts still need: time and the air burnt firing.
struct Cost {
    time_s: f32,
    air_kg: f32,
}

/// Cost of `segments` from segment `index`, which is in `phase` at `rate` with `rotation`
/// turned and coasting at `omega`; the later segments start from rest. Excludes the catch,
/// the margin and the landing reserve.
#[allow(clippy::too_many_arguments)]
fn remaining_cost(
    config: &StuntConfig,
    inputs: &PlannerInputs,
    segments: &[Segment],
    index: usize,
    phase: StuntPhase,
    rate: f32,
    rotation: f32,
    omega: f32,
) -> Cost {
    let mut cost = Cost {
        time_s: 0.0,
        air_kg: 0.0,
    };
    for (i, &segment) in segments.iter().enumerate().skip(index) {
        let last = i + 1 == segments.len();
        let end_rate = end_rate_for(config, last);
        let alpha = alpha_for(config, segment.kind, &inputs.state);
        let air_rate = air_rate_for(config, segment.kind);
        if i == index {
            let target = f32::from(segment.turns) * TAU;
            let brake_s = (rate.max(end_rate) - end_rate) / alpha;
            let (time_s, firing_s) = match phase {
                // Past the point of braking the rotation is what it is.
                StuntPhase::Brake => (brake_s, brake_s),
                StuntPhase::Coast => {
                    let to_turn = (target - rotation - ramp_angle(end_rate, rate, alpha)).max(0.0);
                    let firing_s =
                        (omega - rate).max(0.0) / alpha + (omega - end_rate).max(0.0) / alpha;
                    (to_turn / rate.max(0.1) + brake_s, firing_s)
                }
                _ => {
                    let t = times_to_go(segment, omega, alpha, end_rate, rate, rotation);
                    (t.up_s + t.coast_s + t.brake_s, t.up_s + t.brake_s)
                }
            };
            cost.time_s += time_s;
            cost.air_kg += air_rate * firing_s;
        } else {
            let omega = coast_rate_for(
                config,
                segment.kind,
                inputs.mode,
                alpha,
                segment.turns,
                end_rate,
            );
            let t = times_to_go(segment, omega, alpha, end_rate, 0.0, 0.0);
            cost.time_s += t.up_s + t.coast_s + t.brake_s;
            cost.air_kg += air_rate * (t.up_s + t.brake_s);
        }
    }
    cost
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

/// The planner's four authorization rules (research §2.3, plan G4) for one stunt.
pub fn authorize(
    config: &StuntConfig,
    kind: StuntKind,
    inputs: &PlannerInputs,
) -> Result<StuntPlan, StuntRefusal> {
    authorize_chain(config, &[kind], inputs)
}

/// The same rules for up to [`MAX_STUNTS`] stunts flown back to back in one jump: the time
/// and air of every one of them must fit. `Ok` carries the plan, `Err` the first rule that
/// failed, in the order: start envelope (4), landing surface (1), time (2), air (3).
pub fn authorize_chain(
    config: &StuntConfig,
    kinds: &[StuntKind],
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
    let segments = segments_of(kinds);
    let Some(&first) = segments.first() else {
        return refuse(StuntReason::NoLanding);
    };
    let end_rate = end_rate_for(config, segments.len() == 1);
    let alpha = alpha_for(config, first.kind, &inputs.state);
    let omega = coast_rate_for(
        config,
        first.kind,
        inputs.mode,
        alpha,
        first.turns,
        end_rate,
    );
    let first_times = times_to_go(first, omega, alpha, end_rate, 0.0, 0.0);
    let cost = remaining_cost(
        config,
        inputs,
        &segments,
        0,
        StuntPhase::SpinUp,
        0.0,
        0.0,
        omega,
    );
    let required_s = cost.time_s + config.catch_time_s + config.settle_margin_s;
    // Rule 2.
    if landing.time_s - landing.time_uncertainty_s < required_s {
        return refuse(StuntReason::NotEnoughTime);
    }
    // Rule 3.
    let air_needed_kg = if inputs.unlimited_air {
        0.0
    } else {
        config.air_margin * (cost.air_kg + config.catch_air_kg)
            + landing_reserve_kg(config, &inputs.state, &landing)
    };
    if !inputs.unlimited_air && inputs.air_kg < air_needed_kg {
        return refuse(StuntReason::NotEnoughAir);
    }
    Ok(StuntPlan {
        coast_rate_rad_s: omega,
        alpha_rad_s2: alpha,
        spin_up_s: first_times.up_s,
        coast_s: first_times.coast_s,
        brake_s: first_times.brake_s,
        required_s,
        air_needed_kg,
    })
}

/// The air kept for the PD landing after the stunt, in kg: `landing_air_kg_per_m_s` times
/// the fall speed the forecast says the car has at touchdown. A stunt that would eat it is
/// refused (`NotEnoughAir`).
pub fn landing_reserve_kg(
    config: &StuntConfig,
    state: &CarState,
    landing: &LandingForecast,
) -> f32 {
    let fall_m_s = (state.falling_speed_m_s() + GRAVITY_M_S2 * landing.time_s).max(0.0);
    config.landing_air_kg_per_m_s * fall_m_s
}

/// The stunt machine for one jump. Make one per car, call `request` when the player presses
/// a stunt key (up to [`MAX_STUNTS`] times in a jump) and `tick` every control tick.
#[derive(Debug, Clone)]
pub struct StuntPlanner {
    config: StuntConfig,
    phase: StuntPhase,
    /// The stunt flying now (the kind of the current segment).
    kind: StuntKind,
    requested_at_s: f32,
    settled_at_s: f32,
    /// Stunts asked for and not yet judged, in press order.
    pending: Vec<StuntKind>,
    /// The authorized stunts as spins: neighbours of one kind are one segment.
    segments: Vec<Segment>,
    /// The segment being flown.
    segment_index: usize,
    /// Rotation about the stunt axis in this segment, in the stunt's direction, rad.
    rotation_rad: f32,
    /// Rotation of the segments already flown, rad.
    rotation_before_rad: f32,
    /// Full turns of this segment latched so far: each counts once the integrated rotation
    /// first reaches a full turn (minus the slack), so PD's catch turning the car back
    /// towards level cannot un-complete a turn already made.
    turns_done: u8,
    /// Turns (stunts) of the earlier segments, all complete.
    turns_before: u8,
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
            pending: Vec::new(),
            segments: Vec::new(),
            segment_index: 0,
            rotation_rad: 0.0,
            rotation_before_rad: 0.0,
            turns_done: 0,
            turns_before: 0,
            coast_rate_rad_s: 0.0,
        }
    }

    pub fn config(&self) -> &StuntConfig {
        &self.config
    }

    pub fn phase(&self) -> StuntPhase {
        self.phase
    }

    /// The stunt flying or waiting now.
    pub fn kind(&self) -> StuntKind {
        self.kind
    }

    /// Rotation about the stunt axis so far in the current spin, in the stunt's direction, rad.
    pub fn rotation_rad(&self) -> f32 {
        self.rotation_rad
    }

    /// Rotation of the whole jump's stunts so far, rad.
    pub fn total_rotation_rad(&self) -> f32 {
        self.rotation_before_rad + self.rotation_rad
    }

    /// The authorized stunts of this jump, in order (empty until one is authorized).
    pub fn chain(&self) -> Vec<StuntKind> {
        self.segments
            .iter()
            .flat_map(|s| std::iter::repeat_n(s.kind, usize::from(s.turns)))
            .collect()
    }

    /// The chain as one number for the event stream: the stunt codes, the first in the
    /// ones digit (a double back flip is 44, a back flip then a barrel roll right is 24).
    pub fn chain_code(&self) -> f32 {
        let mut value = 0u32;
        let mut scale = 1u32;
        for kind in self.chain() {
            value += u32::from(kind.code()) * scale;
            scale *= 10;
        }
        value as f32
    }

    /// Stunts authorized (each turn is one), plus those still waiting.
    fn stunts_held(&self) -> usize {
        self.segments
            .iter()
            .map(|s| usize::from(s.turns))
            .sum::<usize>()
            + self.pending.len()
    }

    /// True once every authorized stunt has its full turn latched (2π minus the slack), and
    /// stays true for the rest of the jump's stunts: a ramp launch leaves the car nose up,
    /// and PD's catch turning it back to level must not undo a turn already made. Being
    /// upright is not enough, and a chain is complete only when every stunt in it is.
    pub fn rotation_complete(&self) -> bool {
        !self.segments.is_empty()
            && usize::from(self.turns_before + self.turns_done) >= self.chain().len()
    }

    /// True while the Hybrid gate must stay on PD: from Authorized until
    /// `hybrid_hold_after_settled_s` after Settled.
    pub fn hybrid_hold_active(&self) -> bool {
        !matches!(self.phase, StuntPhase::Idle | StuntPhase::Requested)
    }

    /// Queues a stunt. While Idle it starts a request; while a request waits or a stunt is
    /// spinning up, coasting or braking it adds one more, up to [`MAX_STUNTS`] in all (the
    /// planner judges it at the next tick). False when full or the stunt is past braking.
    pub fn request(&mut self, kind: StuntKind, now_s: f32) -> bool {
        match self.phase {
            StuntPhase::Idle => {
                self.pending.clear();
                self.pending.push(kind);
                self.phase = StuntPhase::Requested;
                self.kind = kind;
                self.requested_at_s = now_s;
                true
            }
            StuntPhase::Requested | StuntPhase::SpinUp | StuntPhase::Coast | StuntPhase::Brake
                if self.stunts_held() < MAX_STUNTS =>
            {
                self.pending.push(kind);
                true
            }
            _ => false,
        }
    }

    fn clear_run(&mut self) {
        self.pending.clear();
        self.segments.clear();
        self.segment_index = 0;
        self.rotation_rad = 0.0;
        self.rotation_before_rad = 0.0;
        self.turns_done = 0;
        self.turns_before = 0;
    }

    /// Drops any stunt (respawn or reset).
    pub fn reset(&mut self) {
        self.phase = StuntPhase::Idle;
        self.clear_run();
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
                self.take_added(inputs, &mut events);
                self.tick_flying(inputs, &mut events);
            }
            StuntPhase::Catch | StuntPhase::Aborted => {
                self.track_rotation(inputs);
                self.pending.clear();
                self.tick_recovery(inputs, &mut events);
            }
            StuntPhase::Settled => {
                if inputs.time_s - self.settled_at_s >= self.config.hybrid_hold_after_settled_s {
                    self.phase = StuntPhase::Idle;
                    self.clear_run();
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
        // Try all the waiting stunts, then fewer: the ones that do not fit are refused.
        let mut keep = self.pending.len();
        let mut last_error = None;
        while keep > 0 {
            match authorize_chain(&self.config, &self.pending[..keep], inputs) {
                Ok(plan) => {
                    let kept: Vec<StuntKind> = self.pending[..keep].to_vec();
                    let dropped: Vec<StuntKind> = self.pending[keep..].to_vec();
                    self.begin(&kept, plan.coast_rate_rad_s);
                    for &kind in &kept {
                        events.push(StuntEvent::Started { kind });
                    }
                    if let Some(reason) = last_error {
                        for kind in dropped {
                            events.push(StuntEvent::Refused { kind, reason });
                        }
                    }
                    return;
                }
                Err(StuntRefusal { reason }) => {
                    let waiting =
                        matches!(reason, StuntReason::NotAirborne | StuntReason::TiltedTooFar);
                    if waiting {
                        // Not in the air yet, or a ramp launch still has the car nose up:
                        // keep the request queued (PD is levelling it) until it expires.
                        if inputs.time_s - self.requested_at_s >= self.config.request_timeout_s {
                            self.expire(events);
                        }
                        return;
                    }
                    last_error = Some(reason);
                    keep -= 1;
                }
            }
        }
        let reason = last_error.unwrap_or(StuntReason::NoLanding);
        for kind in std::mem::take(&mut self.pending) {
            events.push(StuntEvent::Refused { kind, reason });
        }
        self.phase = StuntPhase::Idle;
    }

    fn expire(&mut self, events: &mut Vec<StuntEvent>) {
        for kind in std::mem::take(&mut self.pending) {
            events.push(StuntEvent::Refused {
                kind,
                reason: StuntReason::RequestExpired,
            });
        }
        self.phase = StuntPhase::Idle;
    }

    /// Starts the first spin of `kinds`.
    fn begin(&mut self, kinds: &[StuntKind], coast_rate_rad_s: f32) {
        self.pending.clear();
        self.segments = segments_of(kinds);
        self.segment_index = 0;
        self.kind = self.segments[0].kind;
        self.coast_rate_rad_s = coast_rate_rad_s;
        self.rotation_rad = 0.0;
        self.rotation_before_rad = 0.0;
        self.turns_done = 0;
        self.turns_before = 0;
        // Authorized lasts this one tick, then SpinUp: the nozzles are fired from
        // the next tick on, so report SpinUp now.
        self.phase = StuntPhase::SpinUp;
    }

    /// Judges the stunts added in the air, one by one: each needs the time and air of the
    /// whole chain with it. Accepted ones join the chain (`Started`), the rest are refused.
    fn take_added(&mut self, inputs: &PlannerInputs, events: &mut Vec<StuntEvent>) {
        let added = std::mem::take(&mut self.pending);
        for kind in added {
            let mut segments = self.segments.clone();
            let last_index = segments.len() - 1;
            // The same stunt again joins the spin if that spin has not started braking.
            let joins = last_index > self.segment_index
                || matches!(self.phase, StuntPhase::SpinUp | StuntPhase::Coast);
            match segments.last_mut() {
                Some(last) if last.kind == kind && joins => last.turns += 1,
                _ => segments.push(Segment { kind, turns: 1 }),
            }
            let alpha = alpha_for(&self.config, self.kind, &inputs.state);
            let rate = self.progress_rate(inputs);
            // Spinning up, a longer coast may go faster; once coasting the rate is set.
            let omega = if self.phase == StuntPhase::SpinUp {
                let current = segments[self.segment_index];
                let end_rate = end_rate_for(&self.config, self.segment_index + 1 == segments.len());
                coast_rate_for(
                    &self.config,
                    current.kind,
                    inputs.mode,
                    alpha,
                    current.turns,
                    end_rate,
                )
            } else {
                self.coast_rate_rad_s
            };
            match self.problem_for(&segments, inputs, alpha, rate, omega) {
                None => {
                    self.segments = segments;
                    self.coast_rate_rad_s = omega;
                    events.push(StuntEvent::Started { kind });
                }
                Some(reason) => events.push(StuntEvent::Refused { kind, reason }),
            }
        }
    }

    /// Adds this tick's turn about the stunt axis: body rate × dt, in the stunt's direction.
    fn track_rotation(&mut self, inputs: &PlannerInputs) {
        let rate = self.kind.sign() * inputs.state.angular_velocity_body_rad_s[self.kind.axis()];
        self.rotation_rad += rate * inputs.dt_s;
        let turns = ((self.rotation_rad + self.config.complete_rotation_slack_rad) / TAU)
            .floor()
            .max(0.0);
        let cap = self.segments.get(self.segment_index).map_or(1, |s| s.turns);
        self.turns_done = self.turns_done.max((turns as u8).min(cap));
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
        if let Some(reason) =
            self.problem_for(&self.segments, inputs, alpha, rate, self.coast_rate_rad_s)
        {
            self.abort(reason, events);
            return;
        }

        // Phase changes. Brake as soon as the remaining angle reaches the switching curve.
        let segment = self.segments[self.segment_index];
        let last = self.segment_index + 1 == self.segments.len();
        let end_rate = end_rate_for(&cfg, last);
        let remaining = f32::from(segment.turns) * TAU - self.rotation_rad;
        let curve = ramp_angle(end_rate, rate.max(end_rate), alpha) + rate.max(0.0) * VALVE_LAG_S;
        match self.phase {
            StuntPhase::SpinUp | StuntPhase::Coast if remaining <= curve => {
                self.phase = StuntPhase::Brake;
            }
            StuntPhase::SpinUp if rate >= self.coast_rate_rad_s => self.phase = StuntPhase::Coast,
            _ => {}
        }
        if self.phase == StuntPhase::Brake && rate <= end_rate {
            if last {
                self.phase = StuntPhase::Catch;
            } else if self.turns_done < segment.turns {
                // Stopped short of the turn: the rest of the chain cannot be flown.
                self.abort(StuntReason::NotEnoughTime, events);
            } else {
                self.next_segment(inputs);
            }
        }
    }

    /// Hands the nozzles to the next spin of the chain, the car at rest about the old axis.
    fn next_segment(&mut self, inputs: &PlannerInputs) {
        self.turns_before += self.segments[self.segment_index].turns;
        self.rotation_before_rad += self.rotation_rad;
        self.rotation_rad = 0.0;
        self.turns_done = 0;
        self.segment_index += 1;
        let segment = self.segments[self.segment_index];
        self.kind = segment.kind;
        let alpha = alpha_for(&self.config, segment.kind, &inputs.state);
        let last = self.segment_index + 1 == self.segments.len();
        self.coast_rate_rad_s = coast_rate_for(
            &self.config,
            segment.kind,
            inputs.mode,
            alpha,
            segment.turns,
            end_rate_for(&self.config, last),
        );
        self.phase = StuntPhase::SpinUp;
    }

    /// Why `segments` can no longer be completed from here, if they cannot (replanned every
    /// tick, and run for each stunt added).
    fn problem_for(
        &self,
        segments: &[Segment],
        inputs: &PlannerInputs,
        alpha: f32,
        rate: f32,
        omega: f32,
    ) -> Option<StuntReason> {
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
        let cost = remaining_cost(
            cfg,
            inputs,
            segments,
            self.segment_index,
            self.phase,
            rate,
            self.rotation_rad,
            omega,
        );
        let window_s = landing.time_s - landing.time_uncertainty_s;
        if window_s < cost.time_s + cfg.catch_time_s + cfg.settle_margin_s {
            return Some(StuntReason::NotEnoughTime);
        }
        let braking_last =
            self.phase == StuntPhase::Brake && self.segment_index + 1 == segments.len();
        if !inputs.unlimited_air && !braking_last {
            let needed = cfg.air_margin * (cost.air_kg + cfg.catch_air_kg)
                + landing_reserve_kg(cfg, &inputs.state, &landing);
            if inputs.air_kg < needed {
                return Some(StuntReason::NotEnoughAir);
            }
        }
        None
    }

    fn abort(&mut self, reason: StuntReason, events: &mut Vec<StuntEvent>) {
        self.phase = StuntPhase::Aborted;
        self.pending.clear();
        events.push(StuntEvent::Aborted {
            kind: self.kind,
            reason,
            rotation_rad: self.total_rotation_rad(),
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
        let first = self.chain().first().copied().unwrap_or(self.kind);
        if !aborted && self.rotation_complete() {
            events.push(StuntEvent::Landed {
                kind: first,
                rotation_rad: self.total_rotation_rad(),
            });
        } else if !aborted {
            // Upright, but a rotation was never finished: that is not a stunt.
            events.push(StuntEvent::Aborted {
                kind: self.kind,
                reason: StuntReason::NotEnoughTime,
                rotation_rad: self.total_rotation_rad(),
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
