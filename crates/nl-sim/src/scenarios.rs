//! Training and eval scenarios (SPEC §9.5, §9.6): where and how each episode starts.
//!
//! Stage A drops the car from the air in a random attitude and spin; Stage B lays it on
//! its side just above the ground. Every draw comes from a ChaCha8 generator seeded by the
//! episode's seed, so the same seed always gives the same start (CLAUDE.md rule 2).

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rapier3d::prelude::*;
use serde::Deserialize;

use crate::chaos::random_direction;
use crate::checkpoint::Checkpoint;
use crate::world::{PlaceCarError, Pose, World};

/// The ChaCha8 stream scenario draws use (Chaos uses the default stream 0).
const SCENARIO_RNG_STREAM: u64 = 1;

/// The repo's `configs/scenarios.toml`, built in so tests and the eval runner share it.
pub const DEFAULT_SCENARIOS_TOML: &str = include_str!("../../../configs/scenarios.toml");

/// Which kind of start (SPEC §9.6). Stages C–E arrive with courses in M7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Drop from the air in a random attitude and spin.
    A,
    /// Lying on its side just above the ground.
    B,
}

impl Stage {
    /// "A" or "B" (either case), as on the eval runner's command line.
    pub fn from_name(name: &str) -> Option<Stage> {
        match name {
            "A" | "a" => Some(Stage::A),
            "B" | "b" => Some(Stage::B),
            _ => None,
        }
    }
}

/// When an episode ends (`[episode]`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeConfig {
    /// Success is a save followed by this long with no crash.
    pub success_hold_s: f32,
    /// Episodes are cut off after this long.
    pub time_limit_s: f32,
}

/// Stage A, the drop (`[stage_a]`). Ranges are [low, high].
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropConfig {
    /// Height of the centre of mass above the ground, in m.
    pub height_m: [f32; 2],
    /// Largest tilt from upright, about a random axis, in degrees.
    pub max_tilt_deg: f32,
    /// Largest spin size, about another random axis, in rad/s.
    pub max_spin_rad_s: f32,
    /// Horizontal speed, in a random direction, in m/s.
    pub horizontal_speed_m_s: [f32; 2],
    /// Vertical speed in m/s; positive is up.
    pub vertical_speed_m_s: [f32; 2],
}

/// Stage B, the side tip-over (`[stage_b]`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TipOverConfig {
    /// Roll onto either side, in degrees.
    pub roll_deg: [f32; 2],
    /// Largest extra pitch and yaw, each way, in degrees.
    pub max_pitch_yaw_deg: f32,
    /// Height of the centre of mass above the ground, in m.
    pub height_m: f32,
    /// Largest horizontal sliding speed, in m/s.
    pub max_speed_m_s: f32,
}

/// Everything in scenarios.toml.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenariosConfig {
    /// The `[episode]` section.
    pub episode: EpisodeConfig,
    /// The `[stage_a]` section.
    pub stage_a: DropConfig,
    /// The `[stage_b]` section.
    pub stage_b: TipOverConfig,
}

/// Why scenarios.toml could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ScenariosConfigError {
    #[error("scenarios.toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("scenarios.toml: `{field}` must be [low, high] with low ≤ high, but it is {range:?}")]
    BadRange {
        field: &'static str,
        range: [f32; 2],
    },

    #[error("scenarios.toml: `{field}` must be a finite number, zero or more, but it is {value}")]
    Negative { field: &'static str, value: f32 },

    #[error("scenarios.toml: `episode.time_limit_s` must be above zero, but it is {0}")]
    NoTimeLimit(f32),
}

impl ScenariosConfig {
    /// Parses and checks the text of a scenarios.toml.
    pub fn from_toml_str(text: &str) -> Result<ScenariosConfig, ScenariosConfigError> {
        let config: ScenariosConfig = toml::from_str(text)?;
        config.check_ranges()?;
        config.check_numbers()?;
        Ok(config)
    }

    /// Every [low, high] pair must be finite and in order.
    fn check_ranges(&self) -> Result<(), ScenariosConfigError> {
        let ranges = [
            ("stage_a.height_m", self.stage_a.height_m),
            (
                "stage_a.horizontal_speed_m_s",
                self.stage_a.horizontal_speed_m_s,
            ),
            (
                "stage_a.vertical_speed_m_s",
                self.stage_a.vertical_speed_m_s,
            ),
            ("stage_b.roll_deg", self.stage_b.roll_deg),
        ];
        for (field, range) in ranges {
            let [low, high] = range;
            if !low.is_finite() || !high.is_finite() || low > high {
                return Err(ScenariosConfigError::BadRange { field, range });
            }
        }
        Ok(())
    }

    /// Single numbers: none may be negative (a negative maximum would make an empty
    /// draw range), and the time limit must be above zero.
    fn check_numbers(&self) -> Result<(), ScenariosConfigError> {
        let numbers = [
            ("episode.success_hold_s", self.episode.success_hold_s),
            ("stage_a.max_tilt_deg", self.stage_a.max_tilt_deg),
            ("stage_a.max_spin_rad_s", self.stage_a.max_spin_rad_s),
            ("stage_b.max_pitch_yaw_deg", self.stage_b.max_pitch_yaw_deg),
            ("stage_b.height_m", self.stage_b.height_m),
            ("stage_b.max_speed_m_s", self.stage_b.max_speed_m_s),
        ];
        for (field, value) in numbers {
            if !value.is_finite() || value < 0.0 {
                return Err(ScenariosConfigError::Negative { field, value });
            }
        }
        let limit_s = self.episode.time_limit_s;
        if !limit_s.is_finite() || limit_s <= 0.0 {
            return Err(ScenariosConfigError::NoTimeLimit(limit_s));
        }
        Ok(())
    }
}

/// Places the car for a new episode of `stage` drawn from `seed`, with a full tank.
pub fn start_episode(
    world: &mut World,
    stage: Stage,
    config: &ScenariosConfig,
    seed: u64,
) -> Result<(), PlaceCarError> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    // The world seeds Chaos from the same seed; a stream of its own keeps the start's
    // draws from repeating Chaos's knocks.
    rng.set_stream(SCENARIO_RNG_STREAM);
    let start = match stage {
        Stage::A => sample_drop(&config.stage_a, &mut rng),
        Stage::B => sample_tip_over(&config.stage_b, &mut rng),
    };
    world.place_car(
        start.pose.position_m,
        start.pose.rotation_xyzw,
        start.linear_velocity_m_s,
        start.angular_velocity_rad_s,
    )?;
    world.refill_tank();
    Ok(())
}

/// SPEC §9.6 Stage A: height, a tilt about a random axis, a spin about another, and a
/// velocity with a random horizontal heading.
fn sample_drop(config: &DropConfig, rng: &mut ChaCha8Rng) -> Checkpoint {
    let height_m = draw(rng, config.height_m);
    let heading = Rotation::from_rotation_y(rng.random_range(0.0..std::f32::consts::TAU));
    let tilt_rad = rng.random_range(0.0..=config.max_tilt_deg.to_radians());
    let rotation = Rotation::from_axis_angle(random_direction(rng), tilt_rad) * heading;
    let spin = random_direction(rng) * rng.random_range(0.0..=config.max_spin_rad_s);
    let direction_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let horizontal_m_s = draw(rng, config.horizontal_speed_m_s);
    let velocity = Vector::new(
        horizontal_m_s * direction_rad.cos(),
        draw(rng, config.vertical_speed_m_s),
        horizontal_m_s * direction_rad.sin(),
    );
    checkpoint(Vector::new(0.0, height_m, 0.0), rotation, velocity, spin)
}

/// SPEC §9.6 Stage B: lying on either side (a roll about the nose), slightly pitched and
/// yawed, sliding slowly.
fn sample_tip_over(config: &TipOverConfig, rng: &mut ChaCha8Rng) -> Checkpoint {
    let side = if rng.random_range(0.0..1.0) < 0.5 {
        1.0
    } else {
        -1.0
    };
    let roll_rad = side * draw(rng, config.roll_deg).to_radians();
    let small_rad = config.max_pitch_yaw_deg.to_radians();
    let pitch_rad = rng.random_range(-small_rad..=small_rad);
    let yaw_rad = rng.random_range(-small_rad..=small_rad);
    let rotation = Rotation::from_rotation_y(yaw_rad)
        * Rotation::from_rotation_z(pitch_rad)
        * Rotation::from_rotation_x(roll_rad);
    let direction_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let speed_m_s = rng.random_range(0.0..=config.max_speed_m_s);
    let velocity = Vector::new(
        speed_m_s * direction_rad.cos(),
        0.0,
        speed_m_s * direction_rad.sin(),
    );
    let position = Vector::new(0.0, config.height_m, 0.0);
    checkpoint(position, rotation, velocity, Vector::ZERO)
}

/// An even draw from `[low, high]`.
fn draw(rng: &mut ChaCha8Rng, range: [f32; 2]) -> f32 {
    let [low, high] = range;
    rng.random_range(low..=high)
}

/// A start with this pose and these velocities.
fn checkpoint(position: Vector, rotation: Rotation, velocity: Vector, spin: Vector) -> Checkpoint {
    Checkpoint {
        pose: Pose::new(position, rotation),
        linear_velocity_m_s: velocity.to_array(),
        angular_velocity_rad_s: spin.to_array(),
    }
}
