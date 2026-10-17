//! The Rust eval runner (SPEC §10, M4): run a controller through many seeded episodes of
//! one stage and summarise how it did. No Python needed; the M5 Python eval reports the
//! same fields, so the numbers can be compared.

use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::events::{CrashCause, SimEvent};
use nl_sim::input::InputFrame;
use nl_sim::modes::ModeParams;
use nl_sim::scenarios::{ScenariosConfig, Stage, start_episode};
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, PlaceCarError, World};
use serde::Serialize;

use crate::{ControlCtx, Controller};

/// What happened in one episode (SPEC §10's per-episode row).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EpisodeResult {
    pub seed: u64,
    /// A save, then `success_hold_s` with no crash.
    pub success: bool,
    pub saves: u32,
    pub crashes: u32,
    /// Why it crashed, if it did.
    pub crash_cause: Option<String>,
    pub air_used_kg: f32,
    pub peak_tilt_deg: f32,
    /// Spin size just before the first touchdown, if the car landed.
    pub touchdown_spin_rad_s: Option<f32>,
    /// Fall speed just before the first touchdown, if the car landed.
    pub touchdown_fall_m_s: Option<f32>,
    pub duration_s: f32,
}

/// The whole run, summarised (SPEC §10's metrics).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalSummary {
    pub episodes: usize,
    pub success_rate: f32,
    /// Episodes with at least one save.
    pub save_rate: f32,
    pub crash_rate: f32,
    /// Air used per save, over all episodes; None if nothing was saved.
    pub air_per_save_kg: Option<f32>,
    pub touchdown_spin_median_rad_s: Option<f32>,
    pub touchdown_spin_p95_rad_s: Option<f32>,
    pub touchdown_fall_median_m_s: Option<f32>,
    pub touchdown_fall_p95_m_s: Option<f32>,
}

/// Where an episode's numbers build up while it runs.
#[derive(Default)]
struct Tally {
    saves: u32,
    crashes: u32,
    crash_cause: Option<String>,
    first_save_s: Option<f32>,
    peak_tilt_rad: f32,
    touchdown: Option<(f32, f32)>,
}

/// Runs one episode of `stage` from `seed` with `controller`, in a fresh world.
pub fn run_episode(
    vehicle: &VehicleConfig,
    mode: ModeParams,
    scenarios: &ScenariosConfig,
    stage: Stage,
    controller: &mut dyn Controller,
    seed: u64,
) -> Result<EpisodeResult, PlaceCarError> {
    let mut world = World::new_on_flat_ground(vehicle, mode, DEFAULT_GROUND_FRICTION, seed);
    start_episode(&mut world, stage, scenarios, seed)?;
    controller.reset(seed);
    let air_at_start_kg = world.air_used_kg();
    let mut tally = Tally::default();
    let limit_steps = (scenarios.episode.time_limit_s * PHYSICS_HZ as f32).round() as u64;
    let mut steps = 0;
    while steps < limit_steps && !episode_over(&tally, &world, scenarios) {
        step_once(&mut world, controller, &mut tally);
        steps += 1;
    }
    Ok(EpisodeResult {
        seed,
        success: tally.crashes == 0
            && tally.first_save_s.is_some()
            && held_long_enough(&tally, &world, scenarios),
        saves: tally.saves,
        crashes: tally.crashes,
        crash_cause: tally.crash_cause,
        air_used_kg: world.air_used_kg() - air_at_start_kg,
        peak_tilt_deg: tally.peak_tilt_rad.to_degrees(),
        touchdown_spin_rad_s: tally.touchdown.map(|(spin, _)| spin),
        touchdown_fall_m_s: tally.touchdown.map(|(_, fall)| fall),
        duration_s: world.time_s() as f32,
    })
}

/// One physics step: the controller acts at 60 Hz, then the events are tallied.
fn step_once(world: &mut World, controller: &mut dyn Controller, tally: &mut Tally) {
    let before = world.car_state();
    if world.is_control_step() {
        let ctx = ControlCtx {
            input: InputFrame::default(),
            state: before,
        };
        world.set_controller_throttles(controller.act(&ctx));
    }
    world.step(InputFrame::default());
    tally.peak_tilt_rad = tally.peak_tilt_rad.max(world.tilt_rad());
    for event in world.drain_events() {
        match event {
            SimEvent::AirborneEnd if tally.touchdown.is_none() => {
                let spin = length(before.angular_velocity_body_rad_s);
                tally.touchdown = Some((spin, before.falling_speed_m_s()));
            }
            SimEvent::Save { .. } => {
                tally.saves += 1;
                tally.first_save_s.get_or_insert(world.time_s() as f32);
            }
            SimEvent::Crash { cause } => {
                tally.crashes += 1;
                tally.crash_cause = Some(crash_cause_name(cause).to_string());
            }
            _ => {}
        }
    }
}

/// SPEC §9.5: a crash ends the episode, and so does a save held long enough.
fn episode_over(tally: &Tally, world: &World, scenarios: &ScenariosConfig) -> bool {
    tally.crashes > 0 || held_long_enough(tally, world, scenarios)
}

fn held_long_enough(tally: &Tally, world: &World, scenarios: &ScenariosConfig) -> bool {
    match tally.first_save_s {
        Some(save_s) => world.time_s() as f32 - save_s >= scenarios.episode.success_hold_s,
        None => false,
    }
}

/// Summarises many episodes.
pub fn summarise(results: &[EpisodeResult]) -> EvalSummary {
    let episodes = results.len();
    let rate = |count: usize| count as f32 / episodes.max(1) as f32;
    let mut spins = Vec::new();
    let mut falls = Vec::new();
    let mut total_saves = 0;
    let mut total_air_kg = 0.0;
    for result in results {
        spins.extend(result.touchdown_spin_rad_s);
        falls.extend(result.touchdown_fall_m_s);
        total_saves += result.saves;
        total_air_kg += result.air_used_kg;
    }
    EvalSummary {
        episodes,
        success_rate: rate(results.iter().filter(|result| result.success).count()),
        save_rate: rate(results.iter().filter(|result| result.saves > 0).count()),
        crash_rate: rate(results.iter().filter(|result| result.crashes > 0).count()),
        air_per_save_kg: (total_saves > 0).then(|| total_air_kg / total_saves as f32),
        touchdown_spin_median_rad_s: percentile(&mut spins, 0.5),
        touchdown_spin_p95_rad_s: percentile(&mut spins, 0.95),
        touchdown_fall_median_m_s: percentile(&mut falls, 0.5),
        touchdown_fall_p95_m_s: percentile(&mut falls, 0.95),
    }
}

/// The value below which `fraction` of `values` lie (nearest rank); None if empty.
fn percentile(values: &mut [f32], fraction: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let rank = ((values.len() - 1) as f32 * fraction).round() as usize;
    values.get(rank).copied()
}

fn length(vector: [f32; 3]) -> f32 {
    let [x, y, z] = vector;
    (x * x + y * y + z * z).sqrt()
}

/// A crash cause's name in results files.
fn crash_cause_name(cause: CrashCause) -> &'static str {
    match cause {
        CrashCause::HardImpact => "hard_impact",
        CrashCause::UpsideDown => "upside_down",
        CrashCause::Stuck => "stuck",
        CrashCause::OutOfBounds => "out_of_bounds",
    }
}
