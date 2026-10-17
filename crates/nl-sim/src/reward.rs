//! The training reward (SPEC §9.4), computed once per control step from a plain
//! summary of that step, so each term can be tested on numbers. Weights live in
//! `configs/reward.toml`. There is no survival bonus: being alive must not pay, or the
//! policy learns to hover instead of landing (SPEC §9.11).

use serde::Deserialize;

use crate::conventions::NOZZLE_COUNT;

/// The repo's `configs/reward.toml`, built in so tests and training share it.
pub const DEFAULT_REWARD_TOML: &str = include_str!("../../../configs/reward.toml");

/// How much each term counts (`[weights]`). Each multiplies the term of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardWeights {
    /// Multiplies (up·world_up − 1).
    pub tilt: f32,
    /// Multiplies −min(|ω|², cap).
    pub spin: f32,
    /// Multiplies −(air used / tank capacity) when air is limited.
    pub air_limited: f32,
    /// Multiplies −(throttle sum) when air is unlimited.
    pub air_unlimited: f32,
    /// Cost per step while flying longer than the grace time.
    pub airborne: f32,
    /// Multiplies −|u_t − u_(t−1)|².
    pub smoothness: f32,
    /// Multiplies the save reward.
    pub save: f32,
    /// Multiplies the crash penalty.
    pub crash: f32,
}

/// The constants inside the SPEC §9.4 formulas (`[shape]`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardShape {
    /// |ω|² above this is not charged more, in rad²/s².
    pub spin_cap_rad2_s2: f32,
    /// Airtime shorter than this is free, in s.
    pub airborne_grace_s: f32,
    /// The save reward before its quality bonus.
    pub save_base: f32,
    /// The extra save reward for a perfect (still, soft) landing.
    pub save_quality: f32,
    /// Spin at which the save quality reaches 0, in rad/s.
    pub save_spin_scale_rad_s: f32,
    /// Fall speed at which the save quality reaches 0, in m/s.
    pub save_fall_scale_m_s: f32,
    /// What a crash costs (before its weight).
    pub crash_penalty: f32,
}

/// Everything in reward.toml.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardConfig {
    /// The `[weights]` section.
    pub weights: RewardWeights,
    /// The `[shape]` section.
    pub shape: RewardShape,
}

/// Why reward.toml could not be used.
#[derive(Debug, thiserror::Error)]
pub enum RewardConfigError {
    /// The file is not valid TOML or has missing, extra or mistyped fields.
    #[error("reward.toml: {0}")]
    Toml(#[from] toml::de::Error),

    /// A number is NaN, infinite, or below its allowed minimum.
    #[error("reward.toml: `{field}` must be a finite number {rule}, but it is {value}")]
    OutOfRange {
        field: &'static str,
        rule: &'static str,
        value: f32,
    },
}

impl RewardConfig {
    /// Parses and checks the text of a reward.toml. Every number must be finite and
    /// zero or more (the signs are in the formulas, not the config); the two save
    /// scales divide, so they must be above zero.
    pub fn from_toml_str(text: &str) -> Result<RewardConfig, RewardConfigError> {
        let config: RewardConfig = toml::from_str(text)?;
        for (field, value) in config.named_numbers() {
            if !value.is_finite() || value < 0.0 {
                let rule = "zero or more";
                return Err(RewardConfigError::OutOfRange { field, rule, value });
            }
        }
        let divisors = [
            (
                "shape.save_spin_scale_rad_s",
                config.shape.save_spin_scale_rad_s,
            ),
            (
                "shape.save_fall_scale_m_s",
                config.shape.save_fall_scale_m_s,
            ),
        ];
        for (field, value) in divisors {
            if value <= 0.0 {
                let rule = "above zero";
                return Err(RewardConfigError::OutOfRange { field, rule, value });
            }
        }
        Ok(config)
    }

    /// Every number with its name, for the checks above.
    fn named_numbers(&self) -> [(&'static str, f32); 15] {
        let w = &self.weights;
        let s = &self.shape;
        [
            ("weights.tilt", w.tilt),
            ("weights.spin", w.spin),
            ("weights.air_limited", w.air_limited),
            ("weights.air_unlimited", w.air_unlimited),
            ("weights.airborne", w.airborne),
            ("weights.smoothness", w.smoothness),
            ("weights.save", w.save),
            ("weights.crash", w.crash),
            ("shape.spin_cap_rad2_s2", s.spin_cap_rad2_s2),
            ("shape.airborne_grace_s", s.airborne_grace_s),
            ("shape.save_base", s.save_base),
            ("shape.save_quality", s.save_quality),
            ("shape.save_spin_scale_rad_s", s.save_spin_scale_rad_s),
            ("shape.save_fall_scale_m_s", s.save_fall_scale_m_s),
            ("shape.crash_penalty", s.crash_penalty),
        ]
    }
}

/// What happened over one control step: the numbers the reward needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepSummary {
    /// up·world_up at the end of the step: 1 upright, −1 upside down.
    pub up_alignment: f32,
    /// Size of the angular velocity at the end of the step, in rad/s.
    pub spin_rad_s: f32,
    /// Spin size just before the wheels touched down in the upset this step saved, in
    /// rad/s. Only read when `saved`: a Save is confirmed after a hold on the wheels,
    /// when the car is already still, so the landing itself is what q judges.
    pub landing_spin_rad_s: f32,
    /// Fall speed (positive downward) just before that touchdown, in m/s; only read when
    /// `saved`.
    pub landing_fall_m_s: f32,
    /// Air the thrusters used during the step, in kg.
    pub air_used_kg: f32,
    /// The tank's capacity, in kg.
    pub tank_capacity_kg: f32,
    /// True when air is unlimited (then air costs by throttle instead).
    pub unlimited_air: bool,
    /// The throttles the controller asked for this step, 0..1, nozzle order.
    pub throttles: [f32; NOZZLE_COUNT],
    /// The throttles of the step before, 0..1.
    pub previous_throttles: [f32; NOZZLE_COUNT],
    /// How long the car has been flying, in s: nothing (no wheel, body or roof)
    /// touching anything. A car lying on its side is not flying.
    pub airborne_s: f32,
    /// A Save event happened during the step.
    pub saved: bool,
    /// The car crashed during the step.
    pub crashed: bool,
}

/// The reward of one step, term by term (each already weighted), so training can log
/// which term drives the policy.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RewardTerms {
    /// Cost of leaning away from upright (≤ 0).
    pub tilt: f32,
    /// Cost of spinning (≤ 0).
    pub spin: f32,
    /// Cost of the air used (≤ 0).
    pub air: f32,
    /// Cost of a long flight (≤ 0).
    pub airborne: f32,
    /// Cost of jerky throttles (≤ 0).
    pub smoothness: f32,
    /// Reward for a save (≥ 0).
    pub save: f32,
    /// Cost of a crash (≤ 0).
    pub crash: f32,
}

impl RewardTerms {
    /// The term names, in `as_array` order (training logs them under these names).
    pub const NAMES: [&'static str; 7] = [
        "tilt",
        "spin",
        "air",
        "airborne",
        "smoothness",
        "save",
        "crash",
    ];

    /// The terms in `NAMES` order.
    pub fn as_array(&self) -> [f32; 7] {
        [
            self.tilt,
            self.spin,
            self.air,
            self.airborne,
            self.smoothness,
            self.save,
            self.crash,
        ]
    }

    /// The sum of all terms: the reward PPO sees.
    pub fn total(&self) -> f32 {
        self.tilt + self.spin + self.air + self.airborne + self.smoothness + self.save + self.crash
    }
}

/// SPEC §9.4: the reward for one control step, term by term.
pub fn reward_terms(step: &StepSummary, config: &RewardConfig) -> RewardTerms {
    let w = &config.weights;
    let shape = &config.shape;
    let spin_squared = (step.spin_rad_s * step.spin_rad_s).min(shape.spin_cap_rad2_s2);
    let airborne = if step.airborne_s > shape.airborne_grace_s {
        -w.airborne
    } else {
        0.0
    };
    let save = if step.saved {
        w.save * (shape.save_base + shape.save_quality * save_quality(step, shape))
    } else {
        0.0
    };
    let crash = if step.crashed {
        -w.crash * shape.crash_penalty
    } else {
        0.0
    };
    RewardTerms {
        tilt: w.tilt * (step.up_alignment - 1.0),
        spin: -w.spin * spin_squared,
        air: air_term(step, w),
        airborne,
        smoothness: -w.smoothness * squared_change(&step.throttles, &step.previous_throttles),
        save,
        crash,
    }
}

/// SPEC §9.4 air: a share of the tank when air is limited, the throttle sum otherwise.
fn air_term(step: &StepSummary, weights: &RewardWeights) -> f32 {
    if step.unlimited_air {
        let throttle_sum: f32 = step.throttles.iter().sum();
        return -weights.air_unlimited * throttle_sum;
    }
    -weights.air_limited * step.air_used_kg / step.tank_capacity_kg.max(f32::EPSILON)
}

/// SPEC §9.4 save quality q: 1 for a still, soft landing, falling to 0 with the spin or
/// fall speed at touchdown.
fn save_quality(step: &StepSummary, shape: &RewardShape) -> f32 {
    let still = (1.0 - step.landing_spin_rad_s / shape.save_spin_scale_rad_s).clamp(0.0, 1.0);
    let soft = (1.0 - step.landing_fall_m_s / shape.save_fall_scale_m_s).clamp(0.0, 1.0);
    still * soft
}

/// |now − before|², summed over the nozzles.
fn squared_change(now: &[f32; NOZZLE_COUNT], before: &[f32; NOZZLE_COUNT]) -> f32 {
    let mut total = 0.0;
    for index in 0..NOZZLE_COUNT {
        let change = now[index] - before[index];
        total += change * change;
    }
    total
}
