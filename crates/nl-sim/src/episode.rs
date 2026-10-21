//! One training or eval episode (SPEC §9.5): a fresh world started from a scenario, the
//! rules for when it ends, and the per-episode numbers SPEC §10 reports. The Rust eval
//! runner, the M5 Python binding and the browser replay all step episodes through this,
//! so "success", "crash" and "time limit" mean the same thing everywhere. It also applies
//! the episode's domain randomization (SPEC §9.8): the varied car and world, actuation
//! latency and observation noise, so training and eval randomize the same way. It also
//! measures landing quality (PLAN §3.3): hard landings, heading error, speed kept,
//! settle time and air left. On the driving stages (C, D, E) the driver bot's input and
//! the Chaos knocks are applied inside `step`, so every runner drives them the same way.

use std::collections::VecDeque;

use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::car_state::{CarState, Touchdown};
use crate::conventions::{NOZZLE_COUNT, PHYSICS_HZ, WHEEL_COUNT};
use crate::course::CourseSpec;
use crate::driver_bot::{BotView, DriverBot};
use crate::events::{CrashCause, SimEvent};
use crate::input::{BUTTON_CHAOS, InputFrame};
use crate::landing::LandingQuality;
use crate::obs::{Observation, PREV_ACTION_INDEX, observe};
use crate::randomization::{EpisodeVariation, ObsNoise, add_state_noise, noise_rng};
use crate::scenarios::{EpisodeConfig, ScenariosConfig, Stage};
use crate::stage_start::{Goal, StagePlan, StageStart, begin_stage};
use crate::world::{PlaceCarError, World};

/// Where an episode stands after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeStatus {
    /// Still going.
    Running,
    /// The stage's goal was met with no crash (SPEC §9.5): A/B four wheels down for
    /// `success_hold_s` with quiet throttles for `success_quiet_hold_s`; C a save after
    /// the hop; D the finish line.
    Succeeded,
    /// The car crashed; this ends the episode.
    Crashed(CrashCause),
    /// Stage C: the car's body touched the car row it had to hop over, so success is
    /// out of reach; this ends the episode.
    HitObstacle,
    /// Stage C or D: a landing turned the car `max_landing_heading_error_deg` or more
    /// from its take-off heading (PLAN §3.3), so success is out of reach; this ends
    /// the episode.
    BadLanding,
    /// The time limit ran out first.
    TimedOut,
}

/// What happened in one episode (SPEC §10's per-episode row).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EpisodeResult {
    /// The seed the episode was started from.
    pub seed: u64,
    /// The stage's goal was met with no crash (see `EpisodeStatus::Succeeded`).
    pub success: bool,
    /// Save events seen (a recovery from an upset, SPEC §7.9).
    pub saves: u32,
    /// Why it crashed, if it did (a crash ends the episode, so there is at most one).
    pub crash_cause: Option<CrashCause>,
    /// Air the thrusters used over the whole episode, in kg.
    pub air_used_kg: f32,
    /// Air used during the upsets that ended in a save, in kg (from the Save events).
    pub air_used_in_saves_kg: f32,
    /// The largest tilt from upright, in degrees.
    pub peak_tilt_deg: f32,
    /// Spin size just before the car first touched the ground (wheels, body or roof),
    /// in rad/s; None if it never did.
    pub touchdown_spin_rad_s: Option<f32>,
    /// Fall speed just before that first touch, in m/s.
    pub touchdown_fall_m_s: Option<f32>,
    /// How long the episode ran, in seconds.
    pub duration_s: f32,
    /// HardLanding events seen (a wheel bottomed out at touchdown, PLAN §3.3).
    pub hard_landings: u32,
    /// Heading error at the touchdown of the latest save, in degrees; None without a
    /// save or a clear heading.
    pub landing_heading_error_deg: Option<f32>,
    /// Speed kept after the touchdown of the latest save; None without a save or a
    /// fast enough take-off from the wheels.
    pub speed_kept: Option<f32>,
    /// From the first touchdown until the car was settled for good, in s; None if it
    /// never touched down, was not settled at the end, or crashed.
    pub settle_time_s: Option<f32>,
    /// Air left in the tank at the end, in kg; None with unlimited air.
    pub air_left_kg: Option<f32>,
}

/// The numbers an episode gathers while it runs.
#[derive(Default)]
struct Tally {
    saves: u32,
    crash_cause: Option<CrashCause>,
    air_used_in_saves_kg: f32,
    peak_tilt_rad: f32,
    /// The motion just before the first touch.
    touchdown: Option<Touchdown>,
    /// World time of the first touch, in s.
    first_touch_s: Option<f32>,
    /// When the current unbroken run of being settled began.
    settled_since_s: Option<f32>,
    /// When the current unbroken run of quiet throttles began.
    quiet_since_s: Option<f32>,
    /// HardLanding events seen.
    hard_landings: u32,
    /// The landing quality of the latest save.
    last_save_quality: Option<LandingQuality>,
    /// When the current unbroken run of all four wheels down began.
    wheels_down_since_s: Option<f32>,
    /// The landing of the latest save (what the reward's save quality judges).
    last_save_landing: Option<Touchdown>,
    /// A Hop has started (Stage C counts only saves after it).
    hop_started: bool,
    /// Save events after the Hop started.
    saves_after_hop: u32,
    /// The car's body touched a car row (Stage C).
    obstacle_hit: bool,
    /// A judged landing turned too far from its take-off heading (Stage C/D).
    bad_landing: bool,
    /// Chaos knocks applied so far (Stage E).
    knocks_done: usize,
}

/// A running episode: the world plus the numbers gathered so far.
pub struct Episode {
    world: World,
    rules: EpisodeConfig,
    seed: u64,
    limit_steps: u64,
    steps: u64,
    /// The car on the latest step, so the next step knows what it was just before.
    state: CarState,
    /// Anything (wheels, body or roof) touched the ground on the latest step.
    touching: bool,
    tally: Tally,
    /// What success means, the driver, the knocks and the rows not to touch.
    stage: StageStart,
    /// Throttles on their way to the valves, one per control step of latency.
    delayed_throttles: VecDeque<[f32; NOZZLE_COUNT]>,
    /// What the controller asked for on the latest control step (the observation's
    /// `prev_action`, SPEC §9.2).
    commanded_throttles: [f32; NOZZLE_COUNT],
    /// What the valves are being told now (the commanded throttles, `latency` control
    /// steps late).
    applied_throttles: [f32; NOZZLE_COUNT],
    /// Settled needs a spin below this, in rad/s: the save rule's `save_max_spin_rad_s`.
    settle_max_spin_rad_s: f32,
    /// Sensor noise on what controllers see (all zero: none), and its generator.
    obs_noise: ObsNoise,
    noise_rng: ChaCha8Rng,
}

impl Episode {
    /// A fresh world for `variation` (use `EpisodeVariation::none` for no randomization)
    /// with the car placed for `stage` from `seed`: flat ground for Stages A and B, the
    /// Stage C hop track, or a generated Stage D course (Stage E: its base stage's).
    pub fn start(
        variation: &EpisodeVariation,
        scenarios: &ScenariosConfig,
        stage: Stage,
        seed: u64,
    ) -> Result<Episode, PlaceCarError> {
        let plan = StagePlan::draw(variation, scenarios, stage, seed);
        if let Some(course) = plan.course(scenarios, seed)? {
            let world = Episode::world_on_course(variation, &course, seed);
            return Episode::begin(world, variation, scenarios, &plan, Some(&course), seed);
        }
        let world = World::new_on_flat_ground(
            &variation.vehicle,
            variation.mode,
            variation.ground_friction,
            seed,
        );
        Episode::begin(world, variation, scenarios, &plan, None, seed)
    }

    /// Like `start`, but on `course` (SPEC §7.11) instead of flat ground: the course's
    /// floor, pieces and bounds, with the car placed for `stage` from `seed`.
    pub fn start_on_course(
        variation: &EpisodeVariation,
        scenarios: &ScenariosConfig,
        stage: Stage,
        course: &CourseSpec,
        seed: u64,
    ) -> Result<Episode, PlaceCarError> {
        let plan = StagePlan::draw(variation, scenarios, stage, seed);
        let world = Episode::world_on_course(variation, course, seed);
        Episode::begin(world, variation, scenarios, &plan, Some(course), seed)
    }

    /// A fresh world on `course` for `variation`.
    fn world_on_course(variation: &EpisodeVariation, course: &CourseSpec, seed: u64) -> World {
        World::new_on_course(
            &variation.vehicle,
            variation.mode,
            course,
            variation.ground_friction,
            seed,
        )
    }

    /// Places the car for `plan` in `world` (built on `course`, None for flat ground)
    /// and wraps it in a fresh episode. Stages A and B replace a course's own start pose
    /// with the scenario's start; C and D start on the course's start line.
    fn begin(
        mut world: World,
        variation: &EpisodeVariation,
        scenarios: &ScenariosConfig,
        plan: &StagePlan,
        course: Option<&CourseSpec>,
        seed: u64,
    ) -> Result<Episode, PlaceCarError> {
        let wind = scenarios.wind.storm.scaled(variation.wind.strength);
        world.set_wind(wind, variation.wind.direction_yaw_rad);
        let stage_start = begin_stage(&mut world, variation, scenarios, plan, course, seed)?;
        let delayed_throttles = vec![[0.0; NOZZLE_COUNT]; variation.latency_control_steps];
        let limit_s = plan.time_limit_s();
        Ok(Episode {
            state: world.car_state(),
            touching: false,
            world,
            rules: scenarios.episode,
            seed,
            limit_steps: (limit_s * PHYSICS_HZ as f32).round() as u64,
            steps: 0,
            tally: Tally::default(),
            stage: stage_start,
            delayed_throttles: VecDeque::from(delayed_throttles),
            commanded_throttles: [0.0; NOZZLE_COUNT],
            applied_throttles: [0.0; NOZZLE_COUNT],
            settle_max_spin_rad_s: variation.vehicle.events.save_max_spin_rad_s,
            obs_noise: variation.obs_noise,
            noise_rng: noise_rng(seed),
        })
    }

    /// What a controller sees now: the car's state with this episode's sensor noise
    /// (SPEC §9.8), and the observation built from that same state (SPEC §9.2), whose
    /// `prev_action` is what the controller last commanded.
    pub fn controller_view(&mut self) -> (CarState, Observation) {
        let mut state = self.state;
        add_state_noise(&mut state, &self.obs_noise, &mut self.noise_rng);
        let mut observation = observe(&self.world, &state);
        let prev_action = PREV_ACTION_INDEX..PREV_ACTION_INDEX + NOZZLE_COUNT;
        observation[prev_action].copy_from_slice(&self.commanded_throttles);
        (state, observation)
    }

    /// The observation a controller sees now (see `controller_view`).
    pub fn observe(&mut self) -> Observation {
        self.controller_view().1
    }

    /// The world, for controllers (`is_control_step`) and rendering.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The car on the latest step: what a controller sees.
    pub fn car_state(&self) -> CarState {
        self.state
    }

    /// Save events so far.
    pub fn saves(&self) -> u32 {
        self.tally.saves
    }

    /// How the car was moving just before the touchdown that led to the latest save;
    /// None before the first save.
    pub fn last_save_landing(&self) -> Option<Touchdown> {
        self.tally.last_save_landing
    }

    /// HardLanding events so far.
    pub fn hard_landings(&self) -> u32 {
        self.tally.hard_landings
    }

    /// How well the latest save's landing kept the take-off heading and speed (PLAN
    /// §3.3); None before the first save. The Stage C/D rule reads the heading error.
    pub fn last_save_quality(&self) -> Option<LandingQuality> {
        self.tally.last_save_quality
    }

    /// True once the car has touched down but is not settled on the latest step.
    pub fn unsettled_after_touchdown(&self) -> bool {
        self.tally.first_touch_s.is_some() && self.tally.settled_since_s.is_none()
    }

    /// True if anything (a wheel, the body or the roof) touched the ground on the
    /// latest step; false while the car flies.
    pub fn anything_touching(&self) -> bool {
        self.touching
    }

    /// The driver bot, if this episode has one (Stages C, D and E on C or D).
    pub fn driver(&self) -> Option<&DriverBot> {
        self.stage.driver.as_ref()
    }

    /// True once the car's body has touched a car row (Stage C).
    pub fn obstacle_hit(&self) -> bool {
        self.tally.obstacle_hit
    }

    /// One physics step with the controller's latest throttles (0..1, nozzle order).
    /// On a control step they join the latency queue (SPEC §9.8), and the valves get the
    /// ones from `latency` control steps ago. The driver bot (if any) drives and any
    /// Chaos knock due now is applied. Does nothing once the episode has ended.
    pub fn step(&mut self, throttles: [f32; NOZZLE_COUNT]) -> EpisodeStatus {
        if self.status() != EpisodeStatus::Running {
            return self.status();
        }
        if self.world.is_control_step() {
            self.commanded_throttles = throttles;
            self.delayed_throttles.push_back(throttles);
            if let Some(oldest) = self.delayed_throttles.pop_front() {
                self.applied_throttles = oldest;
            }
        }
        let before = self.state;
        let input = self.driver_input();
        self.world.set_controller_throttles(self.applied_throttles);
        self.world.step(input);
        self.steps += 1;
        self.state = self.world.car_state();
        self.tally.peak_tilt_rad = self.tally.peak_tilt_rad.max(self.world.tilt_rad());
        self.track_touchdown(&before);
        self.track_wheels_down();
        self.track_settled();
        self.track_quiet_throttles();
        self.track_hop_and_obstacles();
        for event in self.world.drain_events() {
            self.record(event);
        }
        self.status()
    }

    /// SPEC §9.5: a crash ends the episode, and so do success and a broken stage rule.
    pub fn status(&self) -> EpisodeStatus {
        if let Some(cause) = self.tally.crash_cause {
            return EpisodeStatus::Crashed(cause);
        }
        if self.tally.obstacle_hit {
            return EpisodeStatus::HitObstacle;
        }
        if self.tally.bad_landing {
            return EpisodeStatus::BadLanding;
        }
        if self.goal_met() {
            return EpisodeStatus::Succeeded;
        }
        if self.steps >= self.limit_steps {
            return EpisodeStatus::TimedOut;
        }
        EpisodeStatus::Running
    }

    /// The per-episode row so far (final once `status` is not `Running`).
    pub fn result(&self) -> EpisodeResult {
        EpisodeResult {
            seed: self.seed,
            success: self.status() == EpisodeStatus::Succeeded,
            saves: self.tally.saves,
            crash_cause: self.tally.crash_cause,
            air_used_kg: self.world.air_used_kg(),
            air_used_in_saves_kg: self.tally.air_used_in_saves_kg,
            peak_tilt_deg: self.tally.peak_tilt_rad.to_degrees(),
            touchdown_spin_rad_s: self.tally.touchdown.map(|touchdown| touchdown.spin_rad_s),
            touchdown_fall_m_s: self.tally.touchdown.map(|touchdown| touchdown.fall_m_s),
            duration_s: self.world.time_s() as f32,
            hard_landings: self.tally.hard_landings,
            landing_heading_error_deg: self
                .tally
                .last_save_quality
                .and_then(|quality| quality.heading_error_rad)
                .map(f32::to_degrees),
            speed_kept: self
                .tally
                .last_save_quality
                .and_then(|quality| quality.speed_kept),
            settle_time_s: self.settle_time_s(),
            air_left_kg: (!self.world.mode().unlimited_air).then(|| self.world.air_kg()),
        }
    }

    /// This step's driver input: the bot's (or none), plus the Chaos button on a knock
    /// step. A knock restarts the four-wheels-down hold, so success after knocks means
    /// the car settled after the last one.
    fn driver_input(&mut self) -> InputFrame {
        let mut input = match self.stage.driver.as_mut() {
            Some(bot) => bot.input(&BotView::of(&self.world)),
            None => InputFrame::default(),
        };
        let next_knock = self.stage.knock_steps.get(self.tally.knocks_done);
        if next_knock == Some(&self.steps) {
            input.buttons |= BUTTON_CHAOS;
            self.tally.knocks_done += 1;
            self.tally.wheels_down_since_s = None;
        }
        input
    }

    /// SPEC §9.5 success for this stage (Stage E: only after every knock).
    fn goal_met(&self) -> bool {
        if self.tally.knocks_done < self.stage.knock_steps.len() {
            return false;
        }
        let parked = self.held_on_wheels_long_enough() && self.quiet_long_enough();
        match self.stage.goal {
            Goal::HoldOnWheels => parked,
            Goal::SaveAfterHop => {
                self.tally.saves_after_hop > 0 && (parked || !self.stage.hold_after_knocks)
            }
            Goal::ReachFinish => self.world.is_finished(),
        }
    }

    /// From the first touch until the current settled run began, if the car is settled
    /// and has not crashed (a frozen wreck can look settled).
    fn settle_time_s(&self) -> Option<f32> {
        if self.tally.crash_cause.is_some() {
            return None;
        }
        let first_touch_s = self.tally.first_touch_s?;
        let settled_since_s = self.tally.settled_since_s?;
        Some((settled_since_s - first_touch_s).max(0.0))
    }

    /// The first time anything touches the ground, keeps the spin and fall speed from
    /// `before` this step (after it, the ground has already stopped the car).
    fn track_touchdown(&mut self, before: &CarState) {
        self.touching = self.state.wheels_down() > 0 || self.world.chassis_touching();
        if self.touching && self.tally.touchdown.is_none() {
            self.tally.touchdown = Some(Touchdown::of(before));
            self.tally.first_touch_s = Some(self.world.time_s() as f32);
        }
    }

    /// PLAN §3.3: starts or breaks the run of being settled: four wheels down, spin
    /// below the save rule's limit and little sideways speed.
    fn track_settled(&mut self) {
        let rules = &self.rules;
        let side_speed_m_s = self.state.linear_velocity_body_m_s[2].abs();
        let settled = self.state.wheels_down() == WHEEL_COUNT
            && self.state.spin_rad_s() < self.settle_max_spin_rad_s
            && side_speed_m_s < rules.settle_max_side_speed_m_s;
        let now_s = self.world.time_s() as f32;
        self.tally.settled_since_s = start_or_break_run(self.tally.settled_since_s, settled, now_s);
    }

    /// PLAN §3.3: starts or breaks the run of quiet throttles that success needs: the
    /// thruster interlock is holding the controller off (no air can blow, SPEC §7.4), or
    /// the controller's throttles add up to less than `success_quiet_throttle_sum`.
    fn track_quiet_throttles(&mut self) {
        let sum = quiet_throttle_sum(
            &self.commanded_throttles,
            self.rules.success_quiet_nozzle_deadband,
        );
        let quiet = self.world.thrusters_locked() || sum < self.rules.success_quiet_throttle_sum;
        let now_s = self.world.time_s() as f32;
        self.tally.quiet_since_s = start_or_break_run(self.tally.quiet_since_s, quiet, now_s);
    }

    /// Starts or breaks the run of all four wheels down that success needs.
    fn track_wheels_down(&mut self) {
        let all_down = self.state.wheels_down() == WHEEL_COUNT;
        let now_s = self.world.time_s() as f32;
        let since_s = self.tally.wheels_down_since_s;
        self.tally.wheels_down_since_s = start_or_break_run(since_s, all_down, now_s);
    }

    /// Notes when a Hop starts, and whether the car touches a car row: the body or
    /// roof touching while the car is over a row means it hit the row. Wheel contact
    /// does not count: the suspension rays reach below the wheels, so a hop clearing
    /// the roofs by a few tenths of a metre would otherwise count as a hit.
    fn track_hop_and_obstacles(&mut self) {
        if self.world.hop_boost().hop_active() {
            self.tally.hop_started = true;
        }
        if self.stage.obstacles.is_empty() || !self.world.chassis_touching_obstacle() {
            return;
        }
        let position_m = self.world.chassis_pose().position_m;
        let over_a_row = self
            .stage
            .obstacles
            .iter()
            .any(|zone| zone.contains(position_m));
        if over_a_row {
            self.tally.obstacle_hit = true;
        }
    }

    /// PLAN §3.3 on Stages C and D: a judged landing (a Save with a heading error) that
    /// turned the car `max_landing_heading_error_deg` or more fails the episode.
    fn judge_landing_heading(&mut self, quality: &LandingQuality) {
        if self.stage.goal == Goal::HoldOnWheels {
            return;
        }
        let limit_rad = self.rules.max_landing_heading_error_deg.to_radians();
        let too_far = quality
            .heading_error_rad
            .is_some_and(|error_rad| error_rad >= limit_rad);
        if too_far {
            self.tally.bad_landing = true;
        }
    }

    /// Adds one event to the tally.
    fn record(&mut self, event: SimEvent) {
        match event {
            SimEvent::Save {
                air_used_kg,
                landing,
                quality,
                ..
            } => {
                self.tally.saves += 1;
                self.tally.air_used_in_saves_kg += air_used_kg;
                self.tally.last_save_landing = Some(landing);
                self.tally.last_save_quality = Some(quality);
                if self.tally.hop_started {
                    self.tally.saves_after_hop += 1;
                }
                self.judge_landing_heading(&quality);
            }
            SimEvent::HardLanding { .. } => self.tally.hard_landings += 1,
            SimEvent::Crash { cause } => self.tally.crash_cause = Some(cause),
            _ => {}
        }
    }

    fn held_on_wheels_long_enough(&self) -> bool {
        self.held_for(self.tally.wheels_down_since_s, self.rules.success_hold_s)
    }

    fn quiet_long_enough(&self) -> bool {
        self.held_for(self.tally.quiet_since_s, self.rules.success_quiet_hold_s)
    }

    /// True if a run that began at `since_s` has lasted at least `hold_s` by now.
    fn held_for(&self, since_s: Option<f32>, hold_s: f32) -> bool {
        match since_s {
            Some(since_s) => self.world.time_s() as f32 - since_s >= hold_s,
            None => false,
        }
    }
}

/// The start time of an unbroken run: kept while `holds`, set to `now_s` when a run
/// begins, and None when it breaks.
fn start_or_break_run(since_s: Option<f32>, holds: bool, now_s: f32) -> Option<f32> {
    if !holds {
        return None;
    }
    Some(since_s.unwrap_or(now_s))
}

/// The throttle sum the quiet-throttle rule counts: every throttle at or above
/// `deadband` (near-zero noise isn't idling).
pub fn quiet_throttle_sum(throttles: &[f32; NOZZLE_COUNT], deadband: f32) -> f32 {
    let mut sum = 0.0;
    for throttle in throttles {
        if *throttle >= deadband {
            sum += throttle;
        }
    }
    sum
}
