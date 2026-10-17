//! Fixed-seed evaluation inside wasm (SPEC §10, M6): runs Stage A episodes through
//! `nl_control::eval::run_episode`, the same code the Rust and Python eval runners use,
//! so a Node test can check that the browser build gives the same episodes, bit for bit.

use nl_control::build_controller;
use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::eval::run_episode;
use nl_sim::episode::EpisodeResult;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use wasm_bindgen::prelude::*;

/// Runs `count` Stage A episodes with seeds `first_seed`, +1, … using `controller`
/// ("off", "pd" or "ai", which needs `policy_json`) in `mode` ("realistic" or "arcade"),
/// with the repo's configs and no randomization. Returns the per-episode rows as JSON,
/// with the same field names as `results/eval_*.json`.
#[wasm_bindgen]
pub fn eval_episodes(
    controller: &str,
    mode: &str,
    first_seed: u64,
    count: u32,
    policy_json: Option<String>,
) -> Result<String, JsError> {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?
        .by_name(mode)
        .ok_or_else(|| JsError::new("eval_episodes: mode must be realistic or arcade"))?;
    let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?;
    let mut controller = build_controller(
        controller,
        &vehicle,
        mode,
        DEFAULT_CONTROLLERS_TOML,
        policy_json.as_deref(),
    )?;
    let plain = EpisodeVariation::none(&vehicle, mode);
    let mut rows: Vec<EpisodeResult> = Vec::new();
    for offset in 0..u64::from(count) {
        let seed = first_seed
            .checked_add(offset)
            .ok_or_else(|| JsError::new("eval_episodes: first_seed is too large"))?;
        // run_episode resets the controller, so one serves every episode.
        rows.push(run_episode(
            &plain,
            &scenarios,
            Stage::A,
            &mut controller,
            seed,
        )?);
    }
    Ok(serde_json::to_string(&rows)?)
}
