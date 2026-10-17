//! Choosing a controller by name and running it (SPEC §8, §11.3). The game's T key, the
//! wasm `set_controller` and both eval runners go through this one list, so the names
//! and the cycle order live in one place.

use nl_policy::Policy;
use nl_sim::conventions::NOZZLE_COUNT;

use crate::{Ai, ControlCtx, Controller, Manual, Off, Pd};

/// Every controller there is. A plain enum rather than trait objects (CLAUDE.md: plain
/// constructs). Each variant holds its controller, so controllers that keep state
/// between control steps (PD's warm start) keep it.
#[derive(Debug, Clone, PartialEq)]
pub enum ControllerKind {
    /// No thrusters (Hop and Boost still work).
    Off(Off),
    /// The classical baseline: attitude PD plus braking (SPEC §8). Boxed because it is
    /// much bigger than the others.
    Pd(Box<Pd>),
    /// The trained policy (SPEC §8). Boxed: it holds the network's weights.
    Ai(Box<Ai>),
    /// Debug: keys 1–0 fire nozzles 0–9.
    Manual(Manual),
}

/// The names `from_name` accepts, in the order T cycles through them (SPEC §11.3).
pub const CONTROLLER_NAMES: [&str; 4] = ["off", "pd", "ai", "manual"];

/// What the named controllers are built from: PD for this car, and the loaded policy
/// if there is one. The owner builds it once and never runs it.
#[derive(Debug, Clone, PartialEq)]
pub struct ControllerChoices {
    /// PD built for the car (`Pd::for_vehicle`).
    pub pd: Pd,
    /// The AI's policy; "ai" cannot be chosen without one.
    pub policy: Option<Policy>,
}

/// Why a controller could not be chosen.
#[derive(Debug, thiserror::Error)]
pub enum ControllerChoiceError {
    /// The name is not in `CONTROLLER_NAMES`.
    #[error("unknown controller \"{name}\"; use one of: {}", CONTROLLER_NAMES.join(", "))]
    Unknown { name: String },

    /// "ai" was asked for, but no policy is loaded.
    #[error("the ai controller needs a policy; load models/policy.json first")]
    NoPolicy,
}

impl ControllerKind {
    /// A fresh controller called `name` (see `CONTROLLER_NAMES`), built from `choices`.
    pub fn from_name(
        name: &str,
        choices: &ControllerChoices,
    ) -> Result<ControllerKind, ControllerChoiceError> {
        match name {
            "off" => Ok(ControllerKind::Off(Off)),
            "pd" => Ok(ControllerKind::Pd(Box::new(choices.pd.clone()))),
            "ai" => match &choices.policy {
                Some(policy) => Ok(ControllerKind::Ai(Box::new(Ai::new(policy.clone())))),
                None => Err(ControllerChoiceError::NoPolicy),
            },
            "manual" => Ok(ControllerKind::Manual(Manual)),
            other => Err(ControllerChoiceError::Unknown {
                name: other.to_string(),
            }),
        }
    }

    /// A fresh controller of the kind after this one in the T-key cycle (Off → PD → AI
    /// → Manual), wrapping round; the AI is skipped while no policy is loaded.
    pub fn next(&self, choices: &ControllerChoices) -> ControllerKind {
        let mut index = CONTROLLER_NAMES
            .iter()
            .position(|name| *name == self.name())
            .unwrap_or(0);
        loop {
            index = (index + 1) % CONTROLLER_NAMES.len();
            if let Ok(next) = ControllerKind::from_name(CONTROLLER_NAMES[index], choices) {
                return next;
            }
        }
    }

    /// Its name in `CONTROLLER_NAMES`.
    pub fn name(&self) -> &'static str {
        match self {
            ControllerKind::Off(_) => "off",
            ControllerKind::Pd(_) => "pd",
            ControllerKind::Ai(_) => "ai",
            ControllerKind::Manual(_) => "manual",
        }
    }
}

impl Controller for ControllerKind {
    /// Forgets any state the controller keeps (PD's warm-start throttles).
    fn reset(&mut self, seed: u64) {
        match self {
            ControllerKind::Off(controller) => controller.reset(seed),
            ControllerKind::Pd(controller) => controller.reset(seed),
            ControllerKind::Ai(controller) => controller.reset(seed),
            ControllerKind::Manual(controller) => controller.reset(seed),
        }
    }

    /// The throttles this controller wants for this control step.
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        match self {
            ControllerKind::Off(controller) => controller.act(ctx),
            ControllerKind::Pd(controller) => controller.act(ctx),
            ControllerKind::Ai(controller) => controller.act(ctx),
            ControllerKind::Manual(controller) => controller.act(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
    use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

    use super::*;
    use crate::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};

    /// PD for the repo's car and no policy.
    fn choices_without_policy() -> ControllerChoices {
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic;
        let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
        ControllerChoices {
            pd: Pd::for_vehicle(&vehicle, mode, &config),
            policy: None,
        }
    }

    #[test]
    fn every_name_but_ai_is_accepted_and_named_back_without_a_policy() {
        let choices = choices_without_policy();
        for name in CONTROLLER_NAMES {
            match ControllerKind::from_name(name, &choices) {
                Ok(controller) => assert_eq!(controller.name(), name),
                Err(error) => {
                    assert_eq!(name, "ai");
                    assert!(matches!(error, ControllerChoiceError::NoPolicy));
                }
            }
        }
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_valid_names() {
        let error = ControllerKind::from_name("autopilot", &choices_without_policy()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "unknown controller \"autopilot\"; use one of: off, pd, ai, manual"
        );
    }

    #[test]
    fn without_a_policy_the_cycle_skips_the_ai_and_comes_back() {
        let choices = choices_without_policy();
        let mut controller = ControllerKind::Off(Off);
        let mut seen = Vec::new();

        for _ in 0..3 {
            controller = controller.next(&choices);
            seen.push(controller.name());
        }

        assert_eq!(seen, ["pd", "manual", "off"]);
    }
}
