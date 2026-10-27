//! The Hybrid's hand-over tilt from controllers.toml `[hybrid]` (DECISIONS 2026-10-27):
//! left out, the gate hands over at the car's upset angle as before; set, the AI takes
//! over only past that tilt, so PD keeps the upsets it saves well.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::ControllerChoices;
use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

fn vehicle() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

fn gate_enter_tilt_deg(controllers_toml: &str) -> f32 {
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let choices = ControllerChoices::from_configs(&vehicle(), mode, controllers_toml, None);
    choices.unwrap().hybrid_gate.enter_tilt_deg
}

fn without_hybrid_section() -> String {
    match DEFAULT_CONTROLLERS_TOML.split_once("[hybrid]") {
        Some((before, _)) => before.to_string(),
        None => DEFAULT_CONTROLLERS_TOML.to_string(),
    }
}

#[test]
fn without_a_hybrid_section_the_gate_hands_over_at_the_upset_angle() {
    let toml = without_hybrid_section();

    let enter_tilt_deg = gate_enter_tilt_deg(&toml);

    assert_eq!(enter_tilt_deg, vehicle().events.upset_tilt_deg);
}

#[test]
fn a_hybrid_enter_tilt_moves_the_hand_over() {
    let toml = format!(
        "{}\n[hybrid]\nenter_tilt_deg = 120.0\n",
        without_hybrid_section()
    );

    let enter_tilt_deg = gate_enter_tilt_deg(&toml);

    assert_eq!(enter_tilt_deg, 120.0);
}

#[test]
fn a_hand_over_tilt_outside_0_to_180_degrees_is_refused() {
    for bad in ["0.0", "-10.0", "181.0"] {
        let toml = format!(
            "{}\n[hybrid]\nenter_tilt_deg = {bad}\n",
            without_hybrid_section()
        );

        let error = ControllersConfig::from_toml_str(&toml).unwrap_err();

        assert!(
            error.to_string().contains("hybrid.enter_tilt_deg"),
            "{error}"
        );
    }
}
