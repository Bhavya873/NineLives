//! Domain randomization (SPEC §9.8): every training episode gets its own car and world,
//! drawn from the ranges in `[randomization]` of scenarios.toml, so the policy cannot
//! overfit one exact car. Draws come from the episode seed on a ChaCha8 stream of their
//! own, so an episode seed always gives the same variation (CLAUDE.md rule 2). The
//! `Episode` applies a variation: car, mode, ground and wind at the start, then
//! actuation latency and observation noise while it runs.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use crate::car_state::{CarState, MAX_HEIGHT_M};
use crate::modes::ModeParams;
use crate::scenarios::draw;
use crate::vehicle_config::VehicleConfig;
use crate::world::DEFAULT_GROUND_FRICTION;

/// The ChaCha8 stream for variation draws (Chaos uses 0, scenario starts 1).
const VARIATION_RNG_STREAM: u64 = 2;
/// The ChaCha8 stream for observation noise during the episode.
const NOISE_RNG_STREAM: u64 = 3;

/// The `[randomization]` ranges. Pairs are [low, high], drawn evenly.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomizationConfig {
    /// Multiplies the dry mass.
    pub mass_factor: [f32; 2],
    /// Largest centre-of-mass move along each body axis, in m.
    pub center_of_mass_offset_m: f32,
    /// Multiplies all three moments of inertia (one draw).
    pub inertia_factor: [f32; 2],
    /// Multiplies each nozzle's max force (one draw per nozzle).
    pub max_force_factor: [f32; 2],
    /// The valve time constant, in s.
    pub valve_time_constant_s: [f32; 2],
    /// Whole control steps of actuation delay, low and high inclusive.
    pub latency_control_steps: [u32; 2],
    /// Friction of the ground the body scrapes.
    pub ground_friction: [f32; 2],
    /// Wind strength as a fraction of the Storm level (0 calm, 1 Storm), [low, high].
    pub wind_strength: [f32; 2],
    /// Draw the mode (thrust scale, air, tank) each episode; false keeps the run's mode
    /// preset, for a policy trained for one mode (round 7).
    pub vary_mode: bool,
    /// The mode's thrust scale.
    pub thrust_scale: [f32; 2],
    /// Chance of an episode with unlimited air.
    pub unlimited_air_probability: f32,
    /// Tank size when air is limited, in kg.
    pub tank_capacity_kg: [f32; 2],
    /// The `[randomization.noise]` table: observation noise sizes.
    pub noise: ObsNoise,
}

/// Standard deviations of the Gaussian noise added to observation rows (SPEC §9.8).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObsNoise {
    /// On `gravity_dir_body` (unit vector components).
    pub gravity_dir: f32,
    /// On `ang_vel_body`, in rad/s.
    pub spin_rad_s: f32,
    /// On `lin_vel_body`, in m/s.
    pub speed_m_s: f32,
    /// On `height`, in m.
    pub height_m: f32,
}

/// One episode's car and world.
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeVariation {
    /// The car this episode drives.
    pub vehicle: VehicleConfig,
    /// Its physics preset (thrust scale, tank).
    pub mode: ModeParams,
    /// Friction of the ground.
    pub ground_friction: f32,
    /// Control steps between a controller acting and the valves hearing it.
    pub latency_control_steps: usize,
    /// Sensor noise on what controllers see (all zero: none).
    pub obs_noise: ObsNoise,
    /// The wind: how strong, as a fraction of the Storm level, and which way it blows.
    pub wind: WindDraw,
}

/// One episode's wind (SPEC §9.8): `strength` × the Storm level of `[wind]`, blowing
/// towards `direction_yaw_rad` (0 along world +X, positive turning left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindDraw {
    /// 0 is calm, 1 the full Storm level with its gusts.
    pub strength: f32,
    /// Where the wind blows towards, in rad.
    pub direction_yaw_rad: f32,
}

impl WindDraw {
    /// No wind.
    pub const CALM: WindDraw = WindDraw {
        strength: 0.0,
        direction_yaw_rad: 0.0,
    };
}

impl EpisodeVariation {
    /// No randomization: the given car and mode, default ground, no latency or noise.
    /// What eval and the game use.
    pub fn none(vehicle: &VehicleConfig, mode: ModeParams) -> EpisodeVariation {
        EpisodeVariation {
            vehicle: vehicle.clone(),
            mode,
            ground_friction: DEFAULT_GROUND_FRICTION,
            latency_control_steps: 0,
            obs_noise: ObsNoise::default(),
            wind: WindDraw::CALM,
        }
    }

    /// The variation of the episode with `seed`: drawn from `randomization` when given
    /// (training, `eval --dr`), otherwise none. Every runner picks it this way.
    pub fn for_seed(
        randomization: Option<&RandomizationConfig>,
        vehicle: &VehicleConfig,
        mode: ModeParams,
        seed: u64,
    ) -> EpisodeVariation {
        match randomization {
            Some(config) => sample_variation(config, vehicle, mode, seed),
            None => EpisodeVariation::none(vehicle, mode),
        }
    }
}

/// Draws the variation for the episode with `seed`, starting from the repo's car and
/// `mode` (whose Hop and Boost settings are kept).
pub fn sample_variation(
    config: &RandomizationConfig,
    vehicle: &VehicleConfig,
    mode: ModeParams,
    seed: u64,
) -> EpisodeVariation {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rng.set_stream(VARIATION_RNG_STREAM);
    let [fewest_steps, most_steps] = config.latency_control_steps;
    let varied_vehicle = vary_vehicle(config, vehicle, &mut rng);
    // Drawn even when the mode is kept, so every later draw is the same either way.
    let varied_mode = vary_mode(config, mode, &mut rng);
    EpisodeVariation {
        vehicle: varied_vehicle,
        mode: if config.vary_mode { varied_mode } else { mode },
        ground_friction: draw(&mut rng, config.ground_friction),
        latency_control_steps: rng.random_range(fewest_steps..=most_steps) as usize,
        obs_noise: config.noise,
        wind: WindDraw {
            strength: draw(&mut rng, config.wind_strength),
            direction_yaw_rad: draw(&mut rng, [0.0, std::f32::consts::TAU]),
        },
    }
}

/// The generator for the observation noise of the episode with `seed`.
pub fn noise_rng(seed: u64) -> ChaCha8Rng {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rng.set_stream(NOISE_RNG_STREAM);
    rng
}

/// Adds Gaussian sensor noise (SPEC §9.8) to what a controller sees of the car: gravity
/// direction, spin, velocity and height. The observation is built from the noisy state,
/// so PD and the AI see the same noise. Zero noise leaves a value alone.
pub fn add_state_noise(state: &mut CarState, noise: &ObsNoise, rng: &mut ChaCha8Rng) {
    add_noise(&mut state.gravity_dir_body, noise.gravity_dir, rng);
    add_noise(
        &mut state.angular_velocity_body_rad_s,
        noise.spin_rad_s,
        rng,
    );
    add_noise(&mut state.linear_velocity_body_m_s, noise.speed_m_s, rng);
    let mut height = [state.height_m];
    add_noise(&mut height, noise.height_m, rng);
    state.height_m = height[0].clamp(0.0, MAX_HEIGHT_M);
}

/// Adds `sigma` × a standard normal draw to each value (nothing when sigma is 0).
fn add_noise(values: &mut [f32], sigma: f32, rng: &mut ChaCha8Rng) {
    if sigma == 0.0 {
        return;
    }
    for value in values {
        *value += sigma * gaussian(rng);
    }
}

/// Mass, inertia, centre of mass, nozzle forces and valve lag.
fn vary_vehicle(
    config: &RandomizationConfig,
    vehicle: &VehicleConfig,
    rng: &mut ChaCha8Rng,
) -> VehicleConfig {
    let mut varied = vehicle.clone();
    let chassis = &mut varied.chassis;
    chassis.dry_mass_kg *= draw(rng, config.mass_factor);
    let inertia_factor = draw(rng, config.inertia_factor);
    for moment in &mut chassis.inertia_kg_m2 {
        *moment *= inertia_factor;
    }
    let offset = config.center_of_mass_offset_m;
    for axis in &mut chassis.center_of_mass_offset_m {
        *axis = draw(rng, [-offset, offset]);
    }
    for nozzle in &mut varied.thrusters.nozzles {
        nozzle.max_force_n *= draw(rng, config.max_force_factor);
    }
    varied.thrusters.valve_time_constant_s = draw(rng, config.valve_time_constant_s);
    varied
}

/// Thrust scale, and unlimited air or a tank size.
fn vary_mode(config: &RandomizationConfig, mode: ModeParams, rng: &mut ChaCha8Rng) -> ModeParams {
    let unlimited_air = rng.random_range(0.0..1.0) < config.unlimited_air_probability;
    ModeParams {
        thrust_scale: draw(rng, config.thrust_scale),
        unlimited_air,
        tank_capacity_kg: draw(rng, config.tank_capacity_kg),
        ..mode
    }
}

/// One draw from the standard normal distribution (Box–Muller).
pub(crate) fn gaussian(rng: &mut ChaCha8Rng) -> f32 {
    // 1 − u keeps the logarithm's argument in (0, 1].
    let u1: f32 = 1.0 - rng.random_range(0.0..1.0);
    let u2: f32 = rng.random_range(0.0..1.0);
    // libm keeps the noise bit-identical on every platform (CLAUDE.md rule 2).
    (-2.0 * libm::logf(u1)).sqrt() * libm::cosf(std::f32::consts::TAU * u2)
}
