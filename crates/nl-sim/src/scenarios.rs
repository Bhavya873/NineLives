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
use crate::obs::MAX_THRUST_SCALE;
use crate::randomization::RandomizationConfig;
use crate::world::{PlaceCarError, Pose, World};

/// The ChaCha8 stream scenario draws use (Chaos uses the default stream 0).
const SCENARIO_RNG_STREAM: u64 = 1;

/// The repo's `configs/scenarios.toml`, built in so tests and the eval runner share it.
pub const DEFAULT_SCENARIOS_TOML: &str = include_str!("../../../configs/scenarios.toml");

/// Which kind of start (SPEC §9.6). Stages C–E arrive with courses in M7. Results and
/// training info spell it "A" or "B".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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
    /// Success is all four wheels down for this long, unbroken, with no crash, in s.
    pub success_hold_s: f32,
    /// Episodes are cut off after this long, in s.
    pub time_limit_s: f32,
    /// Every start is lifted so the body's lowest point is at least this far above the
    /// ground (Stage B: exactly this far), in m, so no start begins inside the ground.
    pub spawn_clearance_m: f32,
}

/// Stage A, the drop (`[stage_a]`). Ranges are [low, high].
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropConfig {
    /// Height of the centre of mass above the ground, in m.
    pub height_m: [f32; 2],
    /// Largest tilt from upright, about a random horizontal axis, in degrees.
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
    /// Largest horizontal sliding speed, in m/s.
    pub max_speed_m_s: f32,
}

/// How Stage A gets harder during training (`[curriculum_a]`, SPEC §9.6).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurriculumAConfig {
    /// Number of difficulty levels; level `levels - 1` is the full `[stage_a]`.
    pub levels: u32,
    /// Largest tilt at level 0, in degrees.
    pub first_max_tilt_deg: f32,
    /// Largest spin at level 0, in rad/s.
    pub first_max_spin_rad_s: f32,
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
    /// The `[curriculum_a]` section.
    pub curriculum_a: CurriculumAConfig,
    /// The `[randomization]` section (SPEC §9.8).
    pub randomization: RandomizationConfig,
}

/// Why scenarios.toml could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ScenariosConfigError {
    #[error("scenarios.toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error(
        "scenarios.toml: `{field}` must be [low, high] with low ≤ high (and low ≥ 0 unless it is a vertical speed), but it is {range:?}"
    )]
    BadRange {
        field: &'static str,
        range: [f32; 2],
    },

    #[error("scenarios.toml: `{field}` must be a finite number, zero or more, but it is {value}")]
    Negative { field: &'static str, value: f32 },

    #[error("scenarios.toml: `episode.time_limit_s` must be above zero, but it is {0}")]
    NoTimeLimit(f32),

    #[error("scenarios.toml: `{field}` must be {rule}, but it is {value}")]
    BadCurriculum {
        field: &'static str,
        rule: &'static str,
        value: f32,
    },

    #[error("scenarios.toml: `randomization.{field}` must be {rule}")]
    BadRandomization {
        field: &'static str,
        rule: &'static str,
    },
}

impl ScenariosConfig {
    /// Parses and checks the text of a scenarios.toml.
    pub fn from_toml_str(text: &str) -> Result<ScenariosConfig, ScenariosConfigError> {
        let config: ScenariosConfig = toml::from_str(text)?;
        config.check_ranges()?;
        config.check_numbers()?;
        config.check_curriculum()?;
        config.check_randomization()?;
        Ok(config)
    }

    /// How many curriculum levels `stage` has (1 for a stage without a curriculum).
    pub fn level_count(&self, stage: Stage) -> u32 {
        match stage {
            Stage::A => self.curriculum_a.levels,
            Stage::B => 1,
        }
    }

    /// These scenarios with `stage` at curriculum `level` (0 = easiest). Only Stage A
    /// has levels so far; any other stage gets the config unchanged.
    pub fn at_level(&self, stage: Stage, level: u32) -> ScenariosConfig {
        match stage {
            Stage::A => ScenariosConfig {
                stage_a: self.stage_a_at_level(level),
                ..*self
            },
            Stage::B => *self,
        }
    }

    /// Stage A at curriculum `level` (0 = easiest): the largest tilt and spin move
    /// evenly from the level-0 values to `[stage_a]`'s, which the last level (and any
    /// level above it) uses. Everything else is `[stage_a]` as written (SPEC §9.6).
    pub fn stage_a_at_level(&self, level: u32) -> DropConfig {
        let curriculum = &self.curriculum_a;
        // With a single level (or none) there is no curriculum: always the full stage.
        let progress = if curriculum.levels <= 1 {
            1.0
        } else {
            (level as f32 / (curriculum.levels - 1) as f32).min(1.0)
        };
        let full = self.stage_a;
        DropConfig {
            max_tilt_deg: lerp(curriculum.first_max_tilt_deg, full.max_tilt_deg, progress),
            max_spin_rad_s: lerp(
                curriculum.first_max_spin_rad_s,
                full.max_spin_rad_s,
                progress,
            ),
            ..full
        }
    }

    /// Randomization factors and sizes must be above zero (a zero mass or tank breaks
    /// the physics); the latency range must be in order; the probability in 0..1. The
    /// friction, wind, offset and noise are checked with the other ranges and numbers.
    fn check_randomization(&self) -> Result<(), ScenariosConfigError> {
        let ranges = &self.randomization;
        let positive_ranges = [
            ("mass_factor", ranges.mass_factor),
            ("inertia_factor", ranges.inertia_factor),
            ("max_force_factor", ranges.max_force_factor),
            ("valve_time_constant_s", ranges.valve_time_constant_s),
            ("thrust_scale", ranges.thrust_scale),
            ("tank_capacity_kg", ranges.tank_capacity_kg),
        ];
        for (field, [low, high]) in positive_ranges {
            if !(low.is_finite() && high.is_finite() && 0.0 < low && low <= high) {
                return Err(bad_randomization(field, "[low, high] with 0 < low ≤ high"));
            }
        }
        if ranges.thrust_scale[1] > MAX_THRUST_SCALE {
            return Err(bad_randomization(
                "thrust_scale",
                "no higher than the observation's 3.0",
            ));
        }
        if ranges.latency_control_steps[0] > ranges.latency_control_steps[1] {
            return Err(bad_randomization(
                "latency_control_steps",
                "[low, high] with low ≤ high",
            ));
        }
        if !(0.0..=1.0).contains(&ranges.unlimited_air_probability) {
            return Err(bad_randomization(
                "unlimited_air_probability",
                "between 0 and 1",
            ));
        }
        Ok(())
    }

    /// The curriculum must have a level and start no harder than it ends.
    fn check_curriculum(&self) -> Result<(), ScenariosConfigError> {
        let curriculum = &self.curriculum_a;
        if curriculum.levels == 0 {
            return Err(ScenariosConfigError::BadCurriculum {
                field: "curriculum_a.levels",
                rule: "at least 1",
                value: 0.0,
            });
        }
        let firsts = [
            (
                "curriculum_a.first_max_tilt_deg",
                curriculum.first_max_tilt_deg,
                self.stage_a.max_tilt_deg,
            ),
            (
                "curriculum_a.first_max_spin_rad_s",
                curriculum.first_max_spin_rad_s,
                self.stage_a.max_spin_rad_s,
            ),
        ];
        for (field, value, last) in firsts {
            if !(0.0..=last).contains(&value) {
                let rule = "between 0 and the matching stage_a maximum";
                return Err(ScenariosConfigError::BadCurriculum { field, rule, value });
            }
        }
        Ok(())
    }

    /// Every [low, high] pair must be finite and in order; only the vertical speed may
    /// go below zero (a negative height, speed or roll makes no sense).
    fn check_ranges(&self) -> Result<(), ScenariosConfigError> {
        let a = &self.stage_a;
        let ranges = [
            ("stage_a.height_m", a.height_m, false),
            (
                "stage_a.horizontal_speed_m_s",
                a.horizontal_speed_m_s,
                false,
            ),
            ("stage_a.vertical_speed_m_s", a.vertical_speed_m_s, true),
            ("stage_b.roll_deg", self.stage_b.roll_deg, false),
            (
                "randomization.ground_friction",
                self.randomization.ground_friction,
                false,
            ),
        ];
        for (field, range, may_be_negative) in ranges {
            let [low, high] = range;
            let in_order = low.is_finite() && high.is_finite() && low <= high;
            if !in_order || (!may_be_negative && low < 0.0) {
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
            ("episode.spawn_clearance_m", self.episode.spawn_clearance_m),
            ("stage_a.max_tilt_deg", self.stage_a.max_tilt_deg),
            ("stage_a.max_spin_rad_s", self.stage_a.max_spin_rad_s),
            ("stage_b.max_pitch_yaw_deg", self.stage_b.max_pitch_yaw_deg),
            ("stage_b.max_speed_m_s", self.stage_b.max_speed_m_s),
            (
                "randomization.center_of_mass_offset_m",
                self.randomization.center_of_mass_offset_m,
            ),
            ("randomization.wind_m_s", self.randomization.wind_m_s),
            (
                "randomization.noise.gravity_dir",
                self.randomization.noise.gravity_dir,
            ),
            (
                "randomization.noise.spin_rad_s",
                self.randomization.noise.spin_rad_s,
            ),
            (
                "randomization.noise.speed_m_s",
                self.randomization.noise.speed_m_s,
            ),
            (
                "randomization.noise.height_m",
                self.randomization.noise.height_m,
            ),
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
    place(world, &start, 0.0)?;
    // Raise (Stage B: or lower) the car until its lowest corner sits the clearance
    // above the ground: a tilted car reaches further down than its centre suggests.
    let gap_m = world.lowest_chassis_point_m();
    let clearance_m = config.episode.spawn_clearance_m;
    let lift_m = match stage {
        Stage::A => (clearance_m - gap_m).max(0.0),
        Stage::B => clearance_m - gap_m,
    };
    place(world, &start, lift_m)?;
    world.refill_tank();
    Ok(())
}

/// Puts the car at `start`, raised by `lift_m`.
fn place(world: &mut World, start: &Checkpoint, lift_m: f32) -> Result<(), PlaceCarError> {
    let [x, y, z] = start.pose.position_m;
    world.place_car(
        [x, y + lift_m, z],
        start.pose.rotation_xyzw,
        start.linear_velocity_m_s,
        start.angular_velocity_rad_s,
    )
}

/// SPEC §9.6 Stage A: height, a tilt about a random horizontal axis (so the car's tilt
/// from upright is exactly the drawn angle), a spin about a random axis, and a velocity
/// with a random horizontal heading.
fn sample_drop(config: &DropConfig, rng: &mut ChaCha8Rng) -> Checkpoint {
    let height_m = draw(rng, config.height_m);
    let heading = Rotation::from_rotation_y(rng.random_range(0.0..std::f32::consts::TAU));
    let tilt_axis_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let tilt_axis = Vector::new(tilt_axis_rad.cos(), 0.0, tilt_axis_rad.sin());
    let tilt_rad = rng.random_range(0.0..=config.max_tilt_deg.to_radians());
    let rotation = Rotation::from_axis_angle(tilt_axis, tilt_rad) * heading;
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
/// yawed, sliding slowly. `start_episode` sets the height.
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
    checkpoint(Vector::ZERO, rotation, velocity, Vector::ZERO)
}

fn bad_randomization(field: &'static str, rule: &'static str) -> ScenariosConfigError {
    ScenariosConfigError::BadRandomization { field, rule }
}

/// The value `progress` (0..1) of the way from `first` to `last`.
fn lerp(first: f32, last: f32, progress: f32) -> f32 {
    first + (last - first) * progress
}

/// An even draw from `[low, high]` (also used by domain randomization).
pub(crate) fn draw(rng: &mut ChaCha8Rng, range: [f32; 2]) -> f32 {
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
