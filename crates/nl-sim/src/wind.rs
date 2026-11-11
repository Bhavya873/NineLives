//! Wind (SPEC §7.2): a steady mean wind plus gusts, which the car's air drag acts
//! against. The game picks a level (Off, Breeze, Storm) from `[wind]` in scenarios.toml;
//! training randomizes up to Storm (SPEC §9.8). Gusts are an Ornstein–Uhlenbeck process
//! on the horizontal wind, stepped once per physics step from a seeded ChaCha8 stream of
//! their own, so a seed always gives the same gusts (CONTRIBUTING.md rule 2).

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::randomization::gaussian;
use crate::scenarios::draw;

/// The ChaCha8 stream for gusts and the wind's direction (see the other streams in
/// `scenarios`, `randomization`, `course_gen`, `driver_bot` and `training`).
const WIND_RNG_STREAM: u64 = 9;

/// The game's wind setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindLevel {
    /// No wind.
    Off,
    /// A light wind with small gusts.
    Breeze,
    /// A strong wind with big gusts.
    Storm,
}

impl WindLevel {
    /// "off", "breeze" or "storm" (any case), as the game's settings name them.
    pub fn from_name(name: &str) -> Option<WindLevel> {
        match name.to_ascii_lowercase().as_str() {
            "off" => Some(WindLevel::Off),
            "breeze" => Some(WindLevel::Breeze),
            "storm" => Some(WindLevel::Storm),
            _ => None,
        }
    }
}

/// One wind level: how strong the mean wind is and how much it gusts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindLevelConfig {
    /// Speed of the steady wind, in m/s.
    pub mean_speed_m_s: f32,
    /// Typical size of a gust along each horizontal axis (steady-state standard
    /// deviation), in m/s.
    pub gust_std_m_s: f32,
    /// How long a gust lasts (the process's time constant), in s.
    pub gust_time_constant_s: f32,
}

impl WindLevelConfig {
    /// No wind and no gusts.
    pub fn calm() -> WindLevelConfig {
        WindLevelConfig {
            mean_speed_m_s: 0.0,
            gust_std_m_s: 0.0,
            gust_time_constant_s: 1.0,
        }
    }

    /// This level with its mean speed and gust size times `strength` (0 is calm, 1 is
    /// this level); gusts last as long. Domain randomization uses it (SPEC §9.8).
    pub fn scaled(&self, strength: f32) -> WindLevelConfig {
        WindLevelConfig {
            mean_speed_m_s: self.mean_speed_m_s * strength,
            gust_std_m_s: self.gust_std_m_s * strength,
            gust_time_constant_s: self.gust_time_constant_s,
        }
    }

    /// The name of the first bad number, if any: speeds may not be negative and the
    /// time constant must be above zero.
    pub(crate) fn first_bad_field(&self) -> Option<&'static str> {
        if !(self.mean_speed_m_s.is_finite() && self.mean_speed_m_s >= 0.0) {
            return Some("mean_speed_m_s");
        }
        if !(self.gust_std_m_s.is_finite() && self.gust_std_m_s >= 0.0) {
            return Some("gust_std_m_s");
        }
        if !(self.gust_time_constant_s.is_finite() && self.gust_time_constant_s > 0.0) {
            return Some("gust_time_constant_s");
        }
        None
    }
}

/// The `[wind]` section of scenarios.toml: one table per level.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindConfig {
    /// `[wind.off]`.
    pub off: WindLevelConfig,
    /// `[wind.breeze]`.
    pub breeze: WindLevelConfig,
    /// `[wind.storm]`; training randomizes up to this level.
    pub storm: WindLevelConfig,
}

impl WindConfig {
    /// The settings of `level`.
    pub fn level(&self, level: WindLevel) -> WindLevelConfig {
        match level {
            WindLevel::Off => self.off,
            WindLevel::Breeze => self.breeze,
            WindLevel::Storm => self.storm,
        }
    }
}

/// The wind now: a level, the direction it blows towards, and the current gust.
/// Saved in `World` snapshots, generator included, so a restored world gusts the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wind {
    level: WindLevelConfig,
    /// Where the wind blows towards, as a heading about +Y: 0 along world +X, positive
    /// turning left (towards −Z), like the car's yaw.
    direction_yaw_rad: f32,
    /// The gust on top of the mean wind, along world X and Z, in m/s.
    gust_m_s: [f32; 2],
    rng: ChaCha8Rng,
}

impl Wind {
    /// No wind, with gusts and directions drawn from `seed` once a level is set.
    pub fn calm(seed: u64) -> Wind {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        rng.set_stream(WIND_RNG_STREAM);
        Wind {
            level: WindLevelConfig::calm(),
            direction_yaw_rad: 0.0,
            gust_m_s: [0.0; 2],
            rng,
        }
    }

    /// Sets `level`, blowing towards `direction_yaw_rad` (see `direction_yaw_rad`), and
    /// starts the gust from zero.
    pub fn set(&mut self, level: WindLevelConfig, direction_yaw_rad: f32) {
        self.level = level;
        self.direction_yaw_rad = direction_yaw_rad;
        self.gust_m_s = [0.0; 2];
    }

    /// Sets `level` blowing in a direction drawn from this wind's seed (the game's
    /// wind setting).
    pub fn set_level(&mut self, level: WindLevelConfig) {
        let direction_yaw_rad = draw(&mut self.rng, [0.0, std::f32::consts::TAU]);
        self.set(level, direction_yaw_rad);
    }

    /// Moves the gust on by `dt_s`. Exact Ornstein–Uhlenbeck step, so the gust size is
    /// right at any step length: g ← a·g + σ·√(1 − a²)·N(0, 1), with a = e^(−dt/τ).
    pub fn step(&mut self, dt_s: f32) {
        let sigma = self.level.gust_std_m_s;
        if sigma == 0.0 {
            self.gust_m_s = [0.0; 2];
            return;
        }
        // libm keeps the gusts bit-identical on every platform (CONTRIBUTING.md rule 2).
        let keep = libm::expf(-dt_s / self.level.gust_time_constant_s);
        let kick = sigma * libm::sqrtf(1.0 - keep * keep);
        for gust in &mut self.gust_m_s {
            *gust = keep * *gust + kick * gaussian(&mut self.rng);
        }
    }

    /// The steady part of the wind, in world coordinates, in m/s.
    pub fn mean_velocity_m_s(&self) -> [f32; 3] {
        let speed = self.level.mean_speed_m_s;
        let yaw = self.direction_yaw_rad;
        // Heading yaw points along (cos yaw, 0, −sin yaw), as the car's nose does.
        [speed * libm::cosf(yaw), 0.0, -speed * libm::sinf(yaw)]
    }

    /// The wind now, mean plus gust, in world coordinates, in m/s.
    pub fn velocity_m_s(&self) -> [f32; 3] {
        let [x, y, z] = self.mean_velocity_m_s();
        [x + self.gust_m_s[0], y, z + self.gust_m_s[1]]
    }
}
