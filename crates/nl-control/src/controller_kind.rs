//! Choosing a controller by name and running it (SPEC §8, §11.3). The game's T key and
//! the wasm `set_controller` go through this one list, so the names and the cycle order
//! live in one place.

use nl_sim::conventions::NOZZLE_COUNT;

use crate::{ControlCtx, Controller, Manual, Off, Pd};

/// Every controller there is. A plain enum rather than trait objects (CLAUDE.md: plain
/// constructs); the AI joins it in M5.
/// Each variant holds its controller, so controllers that keep state between control
/// steps (PD's filters, the AI's normaliser) keep it.
#[derive(Debug, Clone, PartialEq)]
pub enum ControllerKind {
    /// No thrusters (Hop and Boost still work).
    Off(Off),
    /// The classical baseline: attitude PD plus braking (SPEC §8). Boxed because it is
    /// much bigger than the others.
    Pd(Box<Pd>),
    /// Debug: keys 1–0 fire nozzles 0–9.
    Manual(Manual),
}

/// The names `from_name` accepts, in the order T cycles through them.
pub const CONTROLLER_NAMES: [&str; 3] = ["off", "pd", "manual"];

/// A controller name that is not in `CONTROLLER_NAMES`.
#[derive(Debug, thiserror::Error)]
#[error("unknown controller \"{name}\"; use one of: {}", CONTROLLER_NAMES.join(", "))]
pub struct UnknownController {
    pub name: String,
}

impl ControllerKind {
    /// The controller called `name` ("off", "pd" or "manual"). "pd" gives a copy of
    /// `pd`, which the caller builds once for its car (`Pd::new`) and never runs.
    pub fn from_name(name: &str, pd: &Pd) -> Result<ControllerKind, UnknownController> {
        match name {
            "off" => Ok(ControllerKind::Off(Off)),
            "pd" => Ok(ControllerKind::Pd(Box::new(pd.clone()))),
            "manual" => Ok(ControllerKind::Manual(Manual)),
            other => Err(UnknownController {
                name: other.to_string(),
            }),
        }
    }

    /// A fresh controller of the kind after this one in the T-key cycle (Off → PD →
    /// Manual), wrapping round. `pd` is as in `from_name`.
    pub fn next(&self, pd: &Pd) -> ControllerKind {
        match self {
            ControllerKind::Off(_) => ControllerKind::Pd(Box::new(pd.clone())),
            ControllerKind::Pd(_) => ControllerKind::Manual(Manual),
            ControllerKind::Manual(_) => ControllerKind::Off(Off),
        }
    }

    /// Its name in `CONTROLLER_NAMES`.
    pub fn name(&self) -> &'static str {
        match self {
            ControllerKind::Off(_) => "off",
            ControllerKind::Pd(_) => "pd",
            ControllerKind::Manual(_) => "manual",
        }
    }

    /// Forgets any state the controller keeps (PD's warm-start throttles).
    pub fn reset(&mut self, seed: u64) {
        match self {
            ControllerKind::Off(controller) => controller.reset(seed),
            ControllerKind::Pd(controller) => controller.reset(seed),
            ControllerKind::Manual(controller) => controller.reset(seed),
        }
    }

    /// The throttles this controller wants for this control step.
    pub fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        match self {
            ControllerKind::Off(controller) => controller.act(ctx),
            ControllerKind::Pd(controller) => controller.act(ctx),
            ControllerKind::Manual(controller) => controller.act(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
    use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
    use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

    use super::*;
    use crate::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};

    /// PD for the repo's car.
    fn pd() -> Pd {
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic;
        let world = World::new_on_flat_ground(&vehicle, mode, DEFAULT_GROUND_FRICTION, 0);
        let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
        Pd::new(&world.car_model(), &config)
    }

    #[test]
    fn every_listed_name_is_accepted_and_named_back() {
        let pd = pd();
        for name in CONTROLLER_NAMES {
            assert_eq!(ControllerKind::from_name(name, &pd).unwrap().name(), name);
        }
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_valid_names() {
        let error = ControllerKind::from_name("autopilot", &pd()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "unknown controller \"autopilot\"; use one of: off, pd, manual"
        );
    }

    #[test]
    fn the_cycle_follows_the_name_list_and_comes_back() {
        let pd = pd();
        let mut controller = ControllerKind::Off(Off);

        for step in 1..=CONTROLLER_NAMES.len() {
            controller = controller.next(&pd);
            let expected = CONTROLLER_NAMES[step % CONTROLLER_NAMES.len()];
            assert_eq!(controller.name(), expected);
        }
    }
}
