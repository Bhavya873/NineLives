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
    /// Cost per HardLanding event (a wheel bottomed out at touchdown, PLAN §3.3).
    pub hard_landing: f32,
    /// Cost per step while unsettled after the first touchdown (PLAN §3.3).
    pub unsettled: f32,
    /// Multiplies the share of the tank left, paid once on success with a limited tank
    /// (PLAN §3.3).
    pub air_left: f32,
    /// Cost per unit of throttle sum while the car is steady on the ground (upright, low,
    /// not falling, barely turning): the AI's side thrust while driving pushed it off the
    /// Stage D track (EXPERIMENTS m7-full-004). Braking a fall is never charged.
    pub ground_thrust: f32,
    /// Paid once on the step an episode succeeds. Without it nothing in training
    /// rewards the quiet-throttle part of success, and the policy idles thrust while
    /// parked (EXPERIMENTS m7-full-002).
    pub success: f32,
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
    /// Heading error at touchdown at which the save quality reaches 0, in degrees.
    pub save_heading_scale_deg: f32,
    /// What a crash costs (before its weight).
    pub crash_penalty: f32,
    /// "Steady on the ground" (reward `ground_thrust`): tilt at most this, in degrees,
    pub steady_max_tilt_deg: f32,
    /// centre of mass at most this high above the ground, in m,
    pub steady_max_height_m: f32,
    /// spinning at most this fast, in rad/s,
    pub steady_max_spin_rad_s: f32,
    /// and moving up or down at most this fast, in m/s, so braking a fall is never charged.
    pub steady_max_vertical_speed_m_s: f32,
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
        let shape = &config.shape;
        let divisors = [
            ("shape.save_spin_scale_rad_s", shape.save_spin_scale_rad_s),
            ("shape.save_fall_scale_m_s", shape.save_fall_scale_m_s),
            ("shape.save_heading_scale_deg", shape.save_heading_scale_deg),
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
    fn named_numbers(&self) -> [(&'static str, f32); 25] {
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
            ("weights.hard_landing", w.hard_landing),
            ("weights.unsettled", w.unsettled),
            ("weights.air_left", w.air_left),
            ("weights.success", w.success),
            ("weights.ground_thrust", w.ground_thrust),
            ("shape.steady_max_tilt_deg", s.steady_max_tilt_deg),
            ("shape.steady_max_height_m", s.steady_max_height_m),
            ("shape.steady_max_spin_rad_s", s.steady_max_spin_rad_s),
            (
                "shape.steady_max_vertical_speed_m_s",
                s.steady_max_vertical_speed_m_s,
            ),
            ("shape.spin_cap_rad2_s2", s.spin_cap_rad2_s2),
            ("shape.airborne_grace_s", s.airborne_grace_s),
            ("shape.save_base", s.save_base),
            ("shape.save_quality", s.save_quality),
            ("shape.save_spin_scale_rad_s", s.save_spin_scale_rad_s),
            ("shape.save_fall_scale_m_s", s.save_fall_scale_m_s),
            ("shape.save_heading_scale_deg", s.save_heading_scale_deg),
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
    /// Heading error at that touchdown, in rad (PLAN §3.3); None if unknown. Only read
    /// when `saved`.
    pub landing_heading_error_rad: Option<f32>,
    /// Speed kept after that touchdown (PLAN §3.3); None if not measured (a slow
    /// take-off or a start in the air). Only read when `saved`.
    pub landing_speed_kept: Option<f32>,
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
    /// HardLanding events during the step.
    pub hard_landings: u32,
    /// After the episode's first touchdown, the car was not settled at the end of the
    /// step (four wheels down, slow spin, little sideways speed; SPEC §9.5).
    pub unsettled: bool,
    /// The episode succeeded on this step (SPEC §9.5).
    pub succeeded: bool,
    /// Share of the tank left at the end of the step, 0..1 (not read with unlimited air,
    /// where there is no tank to save).
    pub air_left_fraction: f32,
    /// Height of the centre of mass above the ground below it, in m.
    pub height_m: f32,
    /// World vertical speed (positive up), in m/s.
    pub vertical_speed_m_s: f32,
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
    /// Cost of hard landings (≤ 0).
    pub hard_landing: f32,
    /// Cost of being unsettled after touching down (≤ 0).
    pub unsettled: f32,
    /// Bonus for the air left at success (≥ 0).
    pub air_left: f32,
    /// Bonus for succeeding (≥ 0).
    pub success: f32,
    /// Cost of thrust while steady on the ground (≤ 0).
    pub ground_thrust: f32,
}

impl RewardTerms {
    /// The term names, in `as_array` order (training logs them under these names).
    pub const NAMES: [&'static str; 12] = [
        "tilt",
        "spin",
        "air",
        "airborne",
        "smoothness",
        "save",
        "crash",
        "hard_landing",
        "unsettled",
        "air_left",
        "success",
        "ground_thrust",
    ];

    /// The terms in `NAMES` order.
    pub fn as_array(&self) -> [f32; 12] {
        [
            self.tilt,
            self.spin,
            self.air,
            self.airborne,
            self.smoothness,
            self.save,
            self.crash,
            self.hard_landing,
            self.unsettled,
            self.air_left,
            self.success,
            self.ground_thrust,
        ]
    }

    /// The sum of all terms: the reward PPO sees.
    pub fn total(&self) -> f32 {
        let mut total = 0.0;
        for term in self.as_array() {
            total += term;
        }
        total
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
        hard_landing: -w.hard_landing * step.hard_landings as f32,
        unsettled: if step.unsettled { -w.unsettled } else { 0.0 },
        air_left: if step.succeeded && !step.unlimited_air {
            w.air_left * step.air_left_fraction.clamp(0.0, 1.0)
        } else {
            0.0
        },
        success: if step.succeeded { w.success } else { 0.0 },
        ground_thrust: if is_steady_on_ground(step, shape) {
            -w.ground_thrust * step.throttles.iter().sum::<f32>()
        } else {
            0.0
        },
    }
}

/// Upright, low, not falling and barely turning: the car sits or drives on the ground,
/// where thrust only pushes it around (reward `ground_thrust`).
fn is_steady_on_ground(step: &StepSummary, shape: &RewardShape) -> bool {
    let upright = step.up_alignment >= libm::cosf(shape.steady_max_tilt_deg.to_radians());
    upright
        && step.height_m <= shape.steady_max_height_m
        && step.spin_rad_s <= shape.steady_max_spin_rad_s
        && step.vertical_speed_m_s.abs() <= shape.steady_max_vertical_speed_m_s
}

/// SPEC §9.4 air: a share of the tank when air is limited, the throttle sum otherwise.
fn air_term(step: &StepSummary, weights: &RewardWeights) -> f32 {
    if step.unlimited_air {
        let throttle_sum: f32 = step.throttles.iter().sum();
        return -weights.air_unlimited * throttle_sum;
    }
    -weights.air_limited * step.air_used_kg / step.tank_capacity_kg.max(f32::EPSILON)
}

/// SPEC §9.4 save quality q: 1 for a still, soft landing facing the take-off heading
/// with its speed kept, falling to 0 with the spin, fall speed or heading error at
/// touchdown, or the speed lost. An unknown heading error or speed kept counts as perfect.
fn save_quality(step: &StepSummary, shape: &RewardShape) -> f32 {
    let still = clamp01(1.0 - step.landing_spin_rad_s / shape.save_spin_scale_rad_s);
    let soft = clamp01(1.0 - step.landing_fall_m_s / shape.save_fall_scale_m_s);
    let heading_scale_rad = shape.save_heading_scale_deg.to_radians();
    let error_rad = step.landing_heading_error_rad.unwrap_or(0.0);
    let facing = clamp01(1.0 - error_rad / heading_scale_rad);
    let kept = clamp01(step.landing_speed_kept.unwrap_or(1.0));
    still * soft * facing * kept
}

/// `value` limited to 0..1.
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
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
