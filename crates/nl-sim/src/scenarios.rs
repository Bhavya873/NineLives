//! Training and eval scenarios (SPEC §9.5, §9.6): where and how each episode starts.
//!
//! Stage A drops the car from the air in a random attitude and spin; Stage B lays it on
//! its side just above the ground. The driving stages C, D and E are set up in
//! `driving_stages`; their settings live here with the rest of scenarios.toml. Every draw
//! comes from a ChaCha8 generator seeded by the episode's seed, so the same seed always
//! gives the same start (CONTRIBUTING.md rule 2).

use std::sync::Arc;

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rapier3d::prelude::*;
use serde::Deserialize;

use crate::chaos::random_direction;
use crate::checkpoint::Checkpoint;
use crate::course_gen::{PieceCatalog, PieceCatalogError};
use crate::driver_bot::DriverBotConfig;
use crate::driving_stages::{ChaosStageConfig, CourseStageConfig, HopStageConfig};
use crate::flight_stage::{FlightStageConfig, FlightWidening};
use crate::obs::MAX_THRUST_SCALE;
use crate::randomization::RandomizationConfig;
use crate::spin_axis::{ScenarioTag, SpinAxis};
use crate::wind::WindConfig;
use crate::world::{PlaceCarError, Pose, World};

/// The ChaCha8 stream scenario draws use (Chaos uses the default stream 0).
const SCENARIO_RNG_STREAM: u64 = 1;
/// The ChaCha8 stream for the widened Stage A draws, apart from the stream above so the
/// plain drops keep their draws.
const DROP_WIDENING_RNG_STREAM: u64 = 13;

/// The repo's `configs/scenarios.toml`, built in so tests and the eval runner share it.
pub const DEFAULT_SCENARIOS_TOML: &str = include_str!("../../../configs/scenarios.toml");

/// Which kind of episode (SPEC §9.6). Results and training info spell it "A" to "E".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Stage {
    /// Drop from the air in a random attitude and spin.
    A,
    /// Lying on its side just above the ground.
    B,
    /// Driving straight, the bot presses Hop (sometimes over a row of parked cars).
    C,
    /// The bot drives a procedural course to its finish.
    D,
    /// A, C or D with Chaos knocks at random times.
    E,
    /// The bot rolls the car onto a strong flip plate: a flight from the wheels.
    F,
}

impl Stage {
    /// "A" to "F" (either case), as on the eval runner's command line.
    pub fn from_name(name: &str) -> Option<Stage> {
        match name {
            "A" | "a" => Some(Stage::A),
            "B" | "b" => Some(Stage::B),
            "C" | "c" => Some(Stage::C),
            "D" | "d" => Some(Stage::D),
            "E" | "e" => Some(Stage::E),
            "F" | "f" => Some(Stage::F),
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
    /// Stage A and B episodes are cut off after this long, in s (C, D and E have their
    /// own limits).
    pub time_limit_s: f32,
    /// Every start is lifted so the body's lowest point is at least this far above the
    /// ground (Stage B: exactly this far), in m, so no start begins inside the ground.
    pub spawn_clearance_m: f32,
    /// Success also needs the controller's throttle sum to stay below this (PLAN §3.3)...
    pub success_quiet_throttle_sum: f32,
    /// ...not counting nozzles whose throttle is below this (near-zero noise isn't idling).
    pub success_quiet_nozzle_deadband: f32,
    /// ...for this long, unbroken, in s, so idle thrust can't count as a finished episode.
    pub success_quiet_hold_s: f32,
    /// The car is settled when all four wheels are down, it spins slower than the save
    /// rule's `events.save_max_spin_rad_s` (vehicle.toml), and slides sideways slower
    /// than this, in m/s (PLAN §3.3 settle time).
    pub settle_max_side_speed_m_s: f32,
    /// Stage C and D success needs every judged landing (a Save after a take-off from
    /// the wheels) to keep the nose within this of its take-off heading, in degrees
    /// (PLAN §3.3); a worse one ends the episode as `BadLanding`.
    pub max_landing_heading_error_deg: f32,
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

/// The widened Stage A draws (`[widening.stage_a]`): some drops are scripted flip or roll
/// entries, a fast spin about one axis from a height with room to stop it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropWidening {
    /// Chance that a drop is a scripted entry (at the full curriculum level only).
    pub scripted_probability: f32,
    /// Scripted entries start at this height of the centre of mass, in m.
    pub height_m: [f32; 2],
    /// The spin of a scripted entry, in rad/s.
    pub spin_rad_s: [f32; 2],
    /// Largest tilt from upright of a scripted entry, in degrees.
    pub tilt_deg: f32,
    /// Scripted entries only happen when `[stage_a]`'s max spin is at least this, so the
    /// curriculum's easy levels stay easy (the full level is 6).
    pub from_max_spin_rad_s: f32,
}

/// `[widening]`: the wider training and eval episodes of Stages A and F. Remove the
/// section or set `enabled = false` for the distribution before the widening (every
/// result under results/ before it was made that way).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WideningConfig {
    /// False keeps the old Stage A and F draws exactly.
    pub enabled: bool,
    /// The `[widening.stage_f]` section.
    pub stage_f: FlightWidening,
    /// The `[widening.stage_a]` section.
    pub stage_a: DropWidening,
}

/// Everything in scenarios.toml, plus the procedural course pieces (pieces.json).
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    /// The `[stage_c]` section.
    pub stage_c: HopStageConfig,
    /// The `[stage_d]` section.
    pub stage_d: CourseStageConfig,
    /// The `[stage_e]` section.
    pub stage_e: ChaosStageConfig,
    /// The `[stage_f]` section.
    pub stage_f: FlightStageConfig,
    /// The `[driver_bot]` section (SPEC §9.7).
    pub driver_bot: DriverBotConfig,
    /// The `[randomization]` section (SPEC §9.8).
    pub randomization: RandomizationConfig,
    /// The `[wind]` section: the game's wind levels (SPEC §7.2).
    pub wind: WindConfig,
    /// The `[widening]` section; absent is the same as `enabled = false`.
    #[serde(default)]
    pub widening: Option<WideningConfig>,
    /// The Stage D course pieces (pieces.json), set with `with_pieces_json`; None uses
    /// the built-in copy. Not part of scenarios.toml.
    #[serde(skip)]
    pub pieces: Option<Arc<PieceCatalog>>,
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

    #[error("scenarios.toml: `{field}` must be above zero, but it is {value}")]
    NotAboveZero { field: &'static str, value: f32 },

    #[error("scenarios.toml: `{field}` must be {rule}")]
    BadDrivingStage {
        field: &'static str,
        rule: &'static str,
    },

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

    #[error(
        "scenarios.toml: `wind.{level}.{field}` must be a finite number, zero or more (the time constant above zero)"
    )]
    BadWind {
        level: &'static str,
        field: &'static str,
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
        config.check_wind()?;
        config.check_driving_stages()?;
        config.check_widening()?;
        Ok(config)
    }

    /// These scenarios with the course pieces read from the text of a pieces.json
    /// (training passes the repo's file, so a run's saved configs are what it used).
    pub fn with_pieces_json(mut self, text: &str) -> Result<ScenariosConfig, PieceCatalogError> {
        self.pieces = Some(Arc::new(PieceCatalog::from_json_str(text)?));
        Ok(self)
    }

    /// The widening settings if they are switched on.
    pub fn widened(&self) -> Option<&WideningConfig> {
        self.widening.as_ref().filter(|widening| widening.enabled)
    }

    /// These scenarios with the widening switched off: the old Stage A and F draws.
    pub fn legacy(&self) -> ScenariosConfig {
        ScenariosConfig {
            widening: None,
            ..self.clone()
        }
    }

    /// The course pieces Stage D draws from: the ones set by `with_pieces_json`, or the
    /// built-in pieces.json.
    pub fn piece_catalog(&self) -> Result<&PieceCatalog, PlaceCarError> {
        match &self.pieces {
            Some(catalog) => Ok(catalog),
            None => PieceCatalog::built_in().map_err(PlaceCarError::BadCourse),
        }
    }

    /// How many curriculum levels `stage` has (1 for a stage without a curriculum).
    pub fn level_count(&self, stage: Stage) -> u32 {
        match stage {
            Stage::A => self.curriculum_a.levels,
            Stage::B | Stage::C | Stage::D | Stage::E | Stage::F => 1,
        }
    }

    /// How long an episode of `stage` may run, in s (SPEC §9.5).
    pub fn time_limit_s(&self, stage: Stage) -> f32 {
        match stage {
            Stage::A | Stage::B => self.episode.time_limit_s,
            Stage::C => self.stage_c.time_limit_s,
            Stage::D => self.stage_d.time_limit_s,
            Stage::E => self.stage_e.time_limit_s,
            Stage::F => self.stage_f.time_limit_s,
        }
    }

    /// These scenarios with `stage` at curriculum `level` (0 = easiest). Only Stage A
    /// has levels so far; any other stage gets the config unchanged.
    pub fn at_level(&self, stage: Stage, level: u32) -> ScenariosConfig {
        match stage {
            Stage::A => ScenariosConfig {
                stage_a: self.stage_a_at_level(level),
                ..self.clone()
            },
            Stage::B | Stage::C | Stage::D | Stage::E | Stage::F => self.clone(),
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
        let [weakest, strongest] = ranges.wind_strength;
        if !(0.0 <= weakest && weakest <= strongest && strongest <= 1.0) {
            return Err(bad_randomization(
                "wind_strength",
                "[low, high] with 0 ≤ low ≤ high ≤ 1 (1 is the Storm level)",
            ));
        }
        Ok(())
    }

    /// Every wind level's speeds must be zero or more and its time constant above zero.
    fn check_wind(&self) -> Result<(), ScenariosConfigError> {
        let levels = [
            ("off", self.wind.off),
            ("breeze", self.wind.breeze),
            ("storm", self.wind.storm),
        ];
        for (level, settings) in levels {
            if let Some(field) = settings.first_bad_field() {
                return Err(ScenariosConfigError::BadWind { level, field });
            }
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
            ("stage_c.speed_m_s", self.stage_c.speed_m_s, false),
            ("stage_c.hop_time_s", self.stage_c.hop_time_s, false),
            (
                "stage_c.car_row_height_m",
                self.stage_c.car_row_height_m,
                false,
            ),
            ("stage_e.knock_time_s", self.stage_e.knock_time_s, false),
            ("stage_f.speed_m_s", self.stage_f.speed_m_s, false),
            ("stage_f.strength", self.stage_f.strength, false),
            (
                "driver_bot.target_speed_m_s",
                self.driver_bot.target_speed_m_s,
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

    /// The widening ranges are in order and its chances are between 0 and 1.
    fn check_widening(&self) -> Result<(), ScenariosConfigError> {
        let Some(widening) = &self.widening else {
            return Ok(());
        };
        let (f, a) = (&widening.stage_f, &widening.stage_a);
        let ranges = [
            ("widening.stage_f.strength", f.strength),
            ("widening.stage_f.mixed_roll_share", f.mixed_roll_share),
            ("widening.stage_f.mixed_pitch_share", f.mixed_pitch_share),
            ("widening.stage_a.height_m", a.height_m),
            ("widening.stage_a.spin_rad_s", a.spin_rad_s),
        ];
        for (field, range) in ranges {
            let [low, high] = range;
            if !(low.is_finite() && high.is_finite() && 0.0 <= low && low <= high) {
                return Err(ScenariosConfigError::BadRange { field, range });
            }
        }
        let chances = [
            ("widening.stage_f.mixed_probability", f.mixed_probability),
            (
                "widening.stage_a.scripted_probability",
                a.scripted_probability,
            ),
        ];
        for (field, value) in chances {
            if !(0.0..=1.0).contains(&value) {
                return Err(ScenariosConfigError::BadDrivingStage {
                    field,
                    rule: "between 0 and 1",
                });
            }
        }
        let numbers = [
            ("widening.stage_f.start_tilt_deg", f.start_tilt_deg),
            ("widening.stage_f.start_yaw_deg", f.start_yaw_deg),
            ("widening.stage_f.start_spin_rad_s", f.start_spin_rad_s),
            ("widening.stage_a.tilt_deg", a.tilt_deg),
            (
                "widening.stage_a.from_max_spin_rad_s",
                a.from_max_spin_rad_s,
            ),
        ];
        for (field, value) in numbers {
            if !(value.is_finite() && value >= 0.0) {
                return Err(ScenariosConfigError::Negative { field, value });
            }
        }
        Ok(())
    }

    /// Single numbers: none may be negative (a negative maximum would make an empty
    /// draw range), and the time limit must be above zero.
    fn check_numbers(&self) -> Result<(), ScenariosConfigError> {
        let bot = &self.driver_bot;
        let numbers = [
            ("episode.success_hold_s", self.episode.success_hold_s),
            ("episode.spawn_clearance_m", self.episode.spawn_clearance_m),
            (
                "episode.success_quiet_throttle_sum",
                self.episode.success_quiet_throttle_sum,
            ),
            (
                "episode.success_quiet_nozzle_deadband",
                self.episode.success_quiet_nozzle_deadband,
            ),
            (
                "episode.success_quiet_hold_s",
                self.episode.success_quiet_hold_s,
            ),
            (
                "episode.settle_max_side_speed_m_s",
                self.episode.settle_max_side_speed_m_s,
            ),
            ("stage_a.max_tilt_deg", self.stage_a.max_tilt_deg),
            ("stage_a.max_spin_rad_s", self.stage_a.max_spin_rad_s),
            ("stage_b.max_pitch_yaw_deg", self.stage_b.max_pitch_yaw_deg),
            ("stage_b.max_speed_m_s", self.stage_b.max_speed_m_s),
            (
                "randomization.center_of_mass_offset_m",
                self.randomization.center_of_mass_offset_m,
            ),
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
            ("stage_c.spare_length_m", self.stage_c.spare_length_m),
            ("stage_c.run_out_m", self.stage_c.run_out_m),
            ("stage_f.plate_ahead_m", self.stage_f.plate_ahead_m),
            (
                "driver_bot.lookahead_per_speed_s",
                bot.lookahead_per_speed_s,
            ),
            ("driver_bot.speed_gain_per_m_s", bot.speed_gain_per_m_s),
            ("driver_bot.steer_noise_sigma", bot.steer_noise_sigma),
            (
                "driver_bot.steer_noise_return_per_s",
                bot.steer_noise_return_per_s,
            ),
            ("driver_bot.jerk_chance_per_s", bot.jerk_chance_per_s),
            ("driver_bot.jerk_size", bot.jerk_size),
            ("driver_bot.jerk_s", bot.jerk_s),
            ("driver_bot.hop_timing_noise_s", bot.hop_timing_noise_s),
            ("driver_bot.hop_clearance_m", bot.hop_clearance_m),
        ];
        for (field, value) in numbers {
            if !value.is_finite() || value < 0.0 {
                return Err(ScenariosConfigError::Negative { field, value });
            }
        }
        let above_zero = [
            ("episode.time_limit_s", self.episode.time_limit_s),
            ("stage_c.time_limit_s", self.stage_c.time_limit_s),
            ("stage_d.time_limit_s", self.stage_d.time_limit_s),
            ("stage_e.time_limit_s", self.stage_e.time_limit_s),
            ("stage_f.time_limit_s", self.stage_f.time_limit_s),
            ("stage_f.half_width_m", self.stage_f.half_width_m),
            ("stage_f.run_out_m", self.stage_f.run_out_m),
            ("stage_d.time_use_fraction", self.stage_d.time_use_fraction),
            (
                "episode.max_landing_heading_error_deg",
                self.episode.max_landing_heading_error_deg,
            ),
            (
                "driver_bot.lookahead_base_m",
                self.driver_bot.lookahead_base_m,
            ),
            // Zero or less would make the bot's turn limit empty (and `clamp` panic).
            (
                "driver_bot.max_turn_accel_m_s2",
                self.driver_bot.max_turn_accel_m_s2,
            ),
        ];
        for (field, value) in above_zero {
            if !value.is_finite() || value <= 0.0 {
                return Err(ScenariosConfigError::NotAboveZero { field, value });
            }
        }
        Ok(())
    }

    /// The Stage C and E counts, probability and weights. The [low, high] ranges and
    /// single numbers are checked with the others in `check_ranges` and `check_numbers`.
    fn check_driving_stages(&self) -> Result<(), ScenariosConfigError> {
        let bad = |field, rule| Err(ScenariosConfigError::BadDrivingStage { field, rule });
        let [fewest_cars, most_cars] = self.stage_c.car_row_count;
        if fewest_cars == 0 || fewest_cars > most_cars {
            return bad("stage_c.car_row_count", "[low, high] with 1 ≤ low ≤ high");
        }
        if !(0.0..=1.0).contains(&self.stage_c.car_row_probability) {
            return bad("stage_c.car_row_probability", "between 0 and 1");
        }
        let [fewest_knocks, most_knocks] = self.stage_e.knock_count;
        if fewest_knocks > most_knocks {
            return bad("stage_e.knock_count", "[low, high] with low ≤ high");
        }
        let weights = self.stage_e.base_stage_weights;
        let weights_ok = weights.iter().all(|w| w.is_finite() && *w >= 0.0);
        if !weights_ok || weights.iter().sum::<f32>() <= 0.0 {
            let rule = "three weights (A, C, D), none negative, not all zero";
            return bad("stage_e.base_stage_weights", rule);
        }
        // The car starts on its wheels, so the success hold runs from t = 0: it must hit
        // the plate (at the slowest speed) before a quiet car could succeed on the ground.
        let slowest_m_s = self.stage_f.speed_m_s[0];
        let half_hold_s = 0.5 * self.episode.success_hold_s;
        if slowest_m_s <= 0.0 || self.stage_f.plate_ahead_m / slowest_m_s > half_hold_s {
            let rule = "close enough that the slowest speed reaches the plate within half of episode.success_hold_s";
            return bad("stage_f.plate_ahead_m", rule);
        }
        if self.driver_bot.target_speed_m_s[0] <= 0.0 {
            let rule = "[low, high] with 0 < low ≤ high";
            return bad("driver_bot.target_speed_m_s", rule);
        }
        Ok(())
    }
}

/// Places the car for a new Stage A or B episode drawn from `seed`, with a full tank.
/// The driving stages C, D and E start through `episode::Episode`, which also builds
/// their course and driver bot; for them this only refills the tank.
pub fn start_episode(
    world: &mut World,
    stage: Stage,
    config: &ScenariosConfig,
    seed: u64,
) -> Result<(), PlaceCarError> {
    start_episode_tagged(world, stage, config, false, seed).map(|_| ())
}

/// Like `start_episode`, and says what the start was. With `widened` (and the widening
/// switched on in `config`) a Stage A drop may be a scripted flip or roll entry.
pub(crate) fn start_episode_tagged(
    world: &mut World,
    stage: Stage,
    config: &ScenariosConfig,
    widened: bool,
    seed: u64,
) -> Result<ScenarioTag, PlaceCarError> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    // The world seeds Chaos from the same seed; a stream of its own keeps the start's
    // draws from repeating Chaos's knocks.
    rng.set_stream(SCENARIO_RNG_STREAM);
    let mut tag = ScenarioTag::default();
    let start = match stage {
        Stage::A => {
            let scripted = config
                .widened()
                .filter(|_| widened)
                .and_then(|widening| scripted_drop(config, &widening.stage_a, seed));
            match scripted {
                Some((start, scripted_tag)) => {
                    tag = scripted_tag;
                    start
                }
                None => sample_drop(&config.stage_a, &mut rng),
            }
        }
        Stage::B => sample_tip_over(&config.stage_b, &mut rng),
        Stage::C | Stage::D | Stage::E | Stage::F => {
            world.refill_tank();
            return Ok(tag);
        }
    };
    place(world, &start, 0.0)?;
    // Raise (Stage B: or lower) the car until its lowest corner sits the clearance
    // above the ground: a tilted car reaches further down than its centre suggests.
    let gap_m = world.lowest_chassis_point_m();
    let clearance_m = config.episode.spawn_clearance_m;
    let lift_m = match stage {
        Stage::B => clearance_m - gap_m,
        _ => (clearance_m - gap_m).max(0.0),
    };
    place(world, &start, lift_m)?;
    world.refill_tank();
    if stage == Stage::A && tag.spin_axis.is_none() {
        // A plain random drop has no preset, only the axis its start spin is mostly about.
        tag.spin_axis = SpinAxis::dominant(world.car_state().angular_velocity_body_rad_s);
    }
    Ok(tag)
}

/// A scripted Stage A entry for `seed` (a flip, a roll or a tumble about a random axis),
/// or None if this drop is a plain one. Drawn from a stream of its own.
fn scripted_drop(
    config: &ScenariosConfig,
    widening: &DropWidening,
    seed: u64,
) -> Option<(Checkpoint, ScenarioTag)> {
    if config.stage_a.max_spin_rad_s < widening.from_max_spin_rad_s {
        return None;
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rng.set_stream(DROP_WIDENING_RNG_STREAM);
    if rng.random_range(0.0..1.0) >= widening.scripted_probability {
        return None;
    }
    let drop = &config.stage_a;
    let rate_rad_s = draw(&mut rng, widening.spin_rad_s);
    let sign = if rng.random_range(0.0..1.0) < 0.5 {
        1.0
    } else {
        -1.0
    };
    let (name, body_spin) = match rng.random_range(0..3) {
        0 => ("scripted_flip", Vector::new(0.0, 0.0, sign * rate_rad_s)),
        1 => ("scripted_roll", Vector::new(sign * rate_rad_s, 0.0, 0.0)),
        _ => ("scripted_tumble", random_direction(&mut rng) * rate_rad_s),
    };
    let height_m = draw(&mut rng, widening.height_m);
    let heading = Rotation::from_rotation_y(rng.random_range(0.0..std::f32::consts::TAU));
    let tilt_axis_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let tilt_axis = Vector::new(libm::cosf(tilt_axis_rad), 0.0, libm::sinf(tilt_axis_rad));
    let tilt_rad = rng.random_range(0.0..=widening.tilt_deg.to_radians());
    let rotation = Rotation::from_axis_angle(tilt_axis, tilt_rad) * heading;
    let direction_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let horizontal_m_s = draw(&mut rng, drop.horizontal_speed_m_s);
    let velocity = Vector::new(
        horizontal_m_s * libm::cosf(direction_rad),
        draw(&mut rng, drop.vertical_speed_m_s),
        horizontal_m_s * libm::sinf(direction_rad),
    );
    let start = checkpoint(
        Vector::new(0.0, height_m, 0.0),
        rotation,
        velocity,
        rotation * body_spin,
    );
    let tag = ScenarioTag {
        preset: Some(name.to_string()),
        strength: Some(rate_rad_s),
        spin_axis: SpinAxis::dominant(body_spin.to_array()),
    };
    Some((start, tag))
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
    let tilt_axis = Vector::new(libm::cosf(tilt_axis_rad), 0.0, libm::sinf(tilt_axis_rad));
    let tilt_rad = rng.random_range(0.0..=config.max_tilt_deg.to_radians());
    let rotation = Rotation::from_axis_angle(tilt_axis, tilt_rad) * heading;
    let spin = random_direction(rng) * rng.random_range(0.0..=config.max_spin_rad_s);
    let direction_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let horizontal_m_s = draw(rng, config.horizontal_speed_m_s);
    let velocity = Vector::new(
        horizontal_m_s * libm::cosf(direction_rad),
        draw(rng, config.vertical_speed_m_s),
        horizontal_m_s * libm::sinf(direction_rad),
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
        speed_m_s * libm::cosf(direction_rad),
        0.0,
        speed_m_s * libm::sinf(direction_rad),
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
