//! Choosing a controller by name and running it (SPEC §8, §11.3). The game's T key, the
//! wasm `set_controller` and both eval runners go through this one list, so the names
//! and the cycle order live in one place.

use std::sync::Arc;

use nl_policy::{Policy, PolicyError};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::modes::ModeParams;
use nl_sim::vehicle_config::VehicleConfig;

use crate::config::{ControllersConfig, ControllersConfigError};
use crate::{Ai, ControlCtx, Controller, Hybrid, HybridGateSettings, Manual, Off, Pd};

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
    /// The AI flies, PD drives (DECISIONS 2026-10-21). Boxed: it holds both.
    Hybrid(Box<Hybrid>),
}

/// The names `from_name` accepts.
pub const CONTROLLER_NAMES: [&str; 5] = ["off", "pd", "ai", "manual", "hybrid"];

/// The controllers the T key cycles through, in order (SPEC §11.3). Manual is left
/// out: it stays a debug controller that only `set_controller("manual")` picks.
pub const CYCLE_NAMES: [&str; 4] = ["off", "pd", "ai", "hybrid"];

/// What the named controllers are built from: PD for this car, and the loaded policy
/// if there is one. The owner builds it once and never runs it.
#[derive(Debug, Clone, PartialEq)]
pub struct ControllerChoices {
    /// PD built for the car (`Pd::for_vehicle`).
    pub pd: Pd,
    /// The AI's policy, shared by every AI built from these choices; "ai" and "hybrid"
    /// cannot be chosen without one.
    pub policy: Option<Arc<Policy>>,
    /// When Hybrid hands the nozzles between PD and the AI (from vehicle.toml `[events]`).
    pub hybrid_gate: HybridGateSettings,
}

impl ControllerChoices {
    /// The choices for the car `vehicle` describes in `mode`: PD with the gains in
    /// `controllers_toml`, and the policy in `policy_json` if given. The game and every
    /// eval runner build their choices this way.
    pub fn from_configs(
        vehicle: &VehicleConfig,
        mode: ModeParams,
        controllers_toml: &str,
        policy_json: Option<&str>,
    ) -> Result<ControllerChoices, BuildControllerError> {
        let controllers = ControllersConfig::from_toml_str(controllers_toml)?;
        let policy = match policy_json {
            Some(text) => Some(Arc::new(Policy::from_json(text)?)),
            None => None,
        };
        Ok(ControllerChoices {
            pd: Pd::for_vehicle(vehicle, mode, &controllers),
            policy,
            hybrid_gate: HybridGateSettings::for_vehicle(vehicle)
                .with_enter_tilt(controllers.hybrid.enter_tilt_deg)
                .with_policy_max_speed(controllers.hybrid.policy_max_speed_m_s),
        })
    }
}

/// Why `ControllerChoices::from_configs` or `build_controller` failed.
#[derive(Debug, thiserror::Error)]
pub enum BuildControllerError {
    /// controllers.toml is invalid.
    #[error(transparent)]
    Controllers(#[from] ControllersConfigError),
    /// The policy file is invalid or made for another observation layout.
    #[error(transparent)]
    Policy(#[from] PolicyError),
    /// The name is unknown, or "ai" was asked for without a policy.
    #[error(transparent)]
    Choice(#[from] ControllerChoiceError),
}

/// The controller called `name` (see `CONTROLLER_NAMES`) for the car `vehicle`
/// describes in `mode`. `policy_json` is read only for "ai" and "hybrid", so a stale
/// policy file never stops PD or Off from being built.
pub fn build_controller(
    name: &str,
    vehicle: &VehicleConfig,
    mode: ModeParams,
    controllers_toml: &str,
    policy_json: Option<&str>,
) -> Result<ControllerKind, BuildControllerError> {
    let needs_policy = name == "ai" || name == "hybrid";
    let policy_json = if needs_policy { policy_json } else { None };
    let choices = ControllerChoices::from_configs(vehicle, mode, controllers_toml, policy_json)?;
    Ok(ControllerKind::from_name(name, &choices)?)
}

/// Why a controller could not be chosen.
#[derive(Debug, thiserror::Error)]
pub enum ControllerChoiceError {
    /// The name is not in `CONTROLLER_NAMES`.
    #[error("unknown controller \"{name}\"; use one of: {}", CONTROLLER_NAMES.join(", "))]
    Unknown { name: String },

    /// "ai" or "hybrid" (`name`) was asked for, but no policy is loaded.
    #[error("the {name} controller needs a policy; load models/policy.json first")]
    NoPolicy { name: &'static str },
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
                Some(policy) => Ok(ControllerKind::Ai(Box::new(Ai::new(Arc::clone(policy))))),
                None => Err(ControllerChoiceError::NoPolicy { name: "ai" }),
            },
            "manual" => Ok(ControllerKind::Manual(Manual)),
            "hybrid" => match &choices.policy {
                Some(policy) => Ok(ControllerKind::Hybrid(Box::new(Hybrid::new(
                    choices.pd.clone(),
                    Ai::new(Arc::clone(policy)),
                    choices.hybrid_gate,
                )))),
                None => Err(ControllerChoiceError::NoPolicy { name: "hybrid" }),
            },
            other => Err(ControllerChoiceError::Unknown {
                name: other.to_string(),
            }),
        }
    }

    /// A fresh controller of the kind after this one in the T-key cycle (Off → PD → AI →
    /// Hybrid), wrapping round; the AI and Hybrid are skipped while no policy is loaded. Manual, which is
    /// not in the cycle, is followed by PD as if it were Off.
    pub fn next(&self, choices: &ControllerChoices) -> ControllerKind {
        let mut index = CYCLE_NAMES
            .iter()
            .position(|name| *name == self.name())
            .unwrap_or(0);
        loop {
            index = (index + 1) % CYCLE_NAMES.len();
            if let Ok(next) = ControllerKind::from_name(CYCLE_NAMES[index], choices) {
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
            ControllerKind::Hybrid(_) => "hybrid",
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
            ControllerKind::Hybrid(controller) => controller.reset(seed),
        }
    }

    /// The throttles this controller wants for this control step.
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        match self {
            ControllerKind::Off(controller) => controller.act(ctx),
            ControllerKind::Pd(controller) => controller.act(ctx),
            ControllerKind::Ai(controller) => controller.act(ctx),
            ControllerKind::Manual(controller) => controller.act(ctx),
            ControllerKind::Hybrid(controller) => controller.act(ctx),
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
            hybrid_gate: HybridGateSettings::for_vehicle(&vehicle),
        }
    }

    #[test]
    fn every_name_but_ai_and_hybrid_is_accepted_and_named_back_without_a_policy() {
        let choices = choices_without_policy();
        for name in CONTROLLER_NAMES {
            match ControllerKind::from_name(name, &choices) {
                Ok(controller) => assert_eq!(controller.name(), name),
                Err(error) => {
                    assert!(name == "ai" || name == "hybrid");
                    assert!(matches!(error, ControllerChoiceError::NoPolicy { .. }));
                }
            }
        }
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_valid_names() {
        let error = ControllerKind::from_name("autopilot", &choices_without_policy()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "unknown controller \"autopilot\"; use one of: off, pd, ai, manual, hybrid"
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

        assert_eq!(seen, ["pd", "off", "pd"]);
    }

    #[test]
    fn the_cycle_leaves_manual_for_pd_and_never_returns_to_it() {
        let choices = choices_without_policy();

        let next = ControllerKind::Manual(Manual).next(&choices);

        assert_eq!(next.name(), "pd");
        assert!(!CYCLE_NAMES.contains(&"manual"));
    }
}
