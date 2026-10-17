//! Choosing a controller by name and running it (SPEC §8, §11.3). The game's T key, the
//! wasm `set_controller` and (from M4) the eval runner all go through this one list, so
//! the names and the cycle order live in one place.

use nl_sim::conventions::NOZZLE_COUNT;

use crate::{ControlCtx, Controller, Manual, Off};

/// Every controller there is. A plain enum rather than trait objects (CLAUDE.md: plain
/// constructs); PD and the AI join it in M4–M5.
/// Each variant holds its controller, so controllers that keep state between control
/// steps (PD's filters, the AI's normaliser) keep it.
#[derive(Debug, Clone, PartialEq)]
pub enum ControllerKind {
    /// No thrusters (Hop and Boost still work).
    Off(Off),
    /// Debug: keys 1–0 fire nozzles 0–9.
    Manual(Manual),
}

/// The names `from_name` accepts, in the order T cycles through them.
pub const CONTROLLER_NAMES: [&str; 2] = ["off", "manual"];

/// A controller name that is not in `CONTROLLER_NAMES`.
#[derive(Debug, thiserror::Error)]
#[error("unknown controller \"{name}\"; use one of: {}", CONTROLLER_NAMES.join(", "))]
pub struct UnknownController {
    pub name: String,
}

impl ControllerKind {
    /// The controller called `name` ("off" or "manual").
    pub fn from_name(name: &str) -> Result<ControllerKind, UnknownController> {
        match name {
            "off" => Ok(ControllerKind::Off(Off)),
            "manual" => Ok(ControllerKind::Manual(Manual)),
            other => Err(UnknownController {
                name: other.to_string(),
            }),
        }
    }

    /// A fresh controller of the kind after this one in the T-key cycle, wrapping round.
    pub fn next(&self) -> ControllerKind {
        match self {
            ControllerKind::Off(_) => ControllerKind::Manual(Manual),
            ControllerKind::Manual(_) => ControllerKind::Off(Off),
        }
    }

    /// Forgets any state the controller keeps (none do yet).
    pub fn reset(&mut self, seed: u64) {
        match self {
            ControllerKind::Off(controller) => controller.reset(seed),
            ControllerKind::Manual(controller) => controller.reset(seed),
        }
    }

    /// The throttles this controller wants for this control step.
    pub fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        match self {
            ControllerKind::Off(controller) => controller.act(ctx),
            ControllerKind::Manual(controller) => controller.act(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_is_accepted() {
        for name in CONTROLLER_NAMES {
            assert!(ControllerKind::from_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_valid_names() {
        let error = ControllerKind::from_name("autopilot").unwrap_err();

        assert_eq!(
            error.to_string(),
            "unknown controller \"autopilot\"; use one of: off, manual"
        );
    }

    #[test]
    fn the_cycle_visits_every_controller_and_comes_back() {
        let start = ControllerKind::Off(Off);

        assert_eq!(start.next(), ControllerKind::Manual(Manual));
        assert_eq!(start.next().next(), start);
    }
}
