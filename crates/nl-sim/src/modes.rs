//! Game modes: the Realistic and Arcade physics presets (SPEC §7.6).
//!
//! Each mode is a plain `ModeParams` value read from `configs/modes.toml`. The world
//! only ever sees one `ModeParams`, so training can also build its own in between the
//! two presets (SPEC §9.8).

use serde::{Deserialize, Serialize};

use crate::conventions::WHEEL_COUNT;

/// The physics settings one mode uses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeParams {
    /// Multiplies every nozzle's max force (1.0 Realistic, 2.0 Arcade).
    pub thrust_scale: f32,
    /// When true the tank never runs out; air used is still counted.
    pub unlimited_air: bool,
    /// How much air a full tank holds.
    pub tank_capacity_kg: f32,
    /// Refill rate while enough wheels touch the ground.
    pub refill_kg_s: f32,
    /// Wheels that must touch the ground for the tank to refill (SPEC §7.5).
    pub refill_min_wheels: usize,
    /// Thrust fades to zero over this last fraction of the tank (SPEC §7.5).
    pub empty_fade_fraction: f32,
    /// Throttle a Hop puts on the four lift nozzles.
    pub hop_ff: f32,
    /// How long a Hop pushes.
    pub hop_s: f32,
    /// Time after a Hop starts before the next one is allowed.
    pub hop_cooldown_s: f32,
    /// A Hop needs at least this much air, unless air is unlimited.
    pub hop_min_air_kg: f32,
    /// Throttle Boost puts on the two boost nozzles while held.
    pub boost_ff: f32,
}

/// Both presets from `configs/modes.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModesConfig {
    /// The `[realistic]` section.
    pub realistic: ModeParams,
    /// The `[arcade]` section.
    pub arcade: ModeParams,
}

/// Why a modes config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ModesConfigError {
    #[error("modes.toml could not be read: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("modes.toml: `{mode}.{field}` is not valid ({reason}), it is {value}")]
    BadValue {
        mode: &'static str,
        field: &'static str,
        reason: &'static str,
        value: f32,
    },
}

/// The text of the repo's `configs/modes.toml`, built into the library.
pub const DEFAULT_MODES_TOML: &str = include_str!("../../../configs/modes.toml");

impl ModesConfig {
    /// Parses and checks the text of a `modes.toml` file.
    pub fn from_toml_str(text: &str) -> Result<Self, ModesConfigError> {
        let config: ModesConfig = toml::from_str(text)?;
        config.realistic.check("realistic")?;
        config.arcade.check("arcade")?;
        Ok(config)
    }

    /// The mode called `name` ("realistic" or "arcade"), as the UI and the eval runner
    /// spell it; None for any other name.
    pub fn by_name(&self, name: &str) -> Option<ModeParams> {
        match name {
            "realistic" => Some(self.realistic),
            "arcade" => Some(self.arcade),
            _ => None,
        }
    }
}

impl ModeParams {
    /// Checks that every number makes sense. `mode` names the section in errors.
    pub fn check(&self, mode: &'static str) -> Result<(), ModesConfigError> {
        let field = FieldChecker { mode };
        field.positive("thrust_scale", self.thrust_scale)?;
        field.positive("tank_capacity_kg", self.tank_capacity_kg)?;
        field.not_negative("refill_kg_s", self.refill_kg_s)?;
        if !(1..=WHEEL_COUNT).contains(&self.refill_min_wheels) {
            let wheels = self.refill_min_wheels as f32;
            return Err(field.bad("refill_min_wheels", "must be 1 to 4", wheels));
        }
        field.positive("empty_fade_fraction", self.empty_fade_fraction)?;
        field.fraction("empty_fade_fraction", self.empty_fade_fraction)?;
        field.fraction("hop_ff", self.hop_ff)?;
        field.not_negative("hop_s", self.hop_s)?;
        field.not_negative("hop_cooldown_s", self.hop_cooldown_s)?;
        field.not_negative("hop_min_air_kg", self.hop_min_air_kg)?;
        field.fraction("boost_ff", self.boost_ff)
    }
}

/// Checks the fields of one mode section and builds errors that name it.
struct FieldChecker {
    mode: &'static str,
}

impl FieldChecker {
    /// Finite and above zero.
    fn positive(&self, field: &'static str, value: f32) -> Result<(), ModesConfigError> {
        if value.is_finite() && value > 0.0 {
            return Ok(());
        }
        Err(self.bad(field, "must be a finite number above zero", value))
    }

    /// Finite and zero or more.
    fn not_negative(&self, field: &'static str, value: f32) -> Result<(), ModesConfigError> {
        if value.is_finite() && value >= 0.0 {
            return Ok(());
        }
        Err(self.bad(field, "must be a finite number, zero or more", value))
    }

    /// Between 0 and 1, inclusive.
    fn fraction(&self, field: &'static str, value: f32) -> Result<(), ModesConfigError> {
        if (0.0..=1.0).contains(&value) {
            return Ok(());
        }
        Err(self.bad(field, "must be between 0 and 1", value))
    }

    fn bad(&self, field: &'static str, reason: &'static str, value: f32) -> ModesConfigError {
        ModesConfigError::BadValue {
            mode: self.mode,
            field,
            reason,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repo_modes_load_with_the_spec_numbers() {
        let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();

        assert_eq!(modes.realistic.thrust_scale, 1.0);
        assert_eq!(modes.arcade.thrust_scale, 2.0);
        assert!(!modes.realistic.unlimited_air);
        assert!(modes.arcade.unlimited_air);
    }

    #[test]
    fn a_hop_throttle_above_one_is_rejected_with_the_field_name() {
        let broken = DEFAULT_MODES_TOML.replacen("hop_ff = 0.75", "hop_ff = 1.5", 1);

        let error = ModesConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("realistic.hop_ff"), "{error}");
    }
}
