//! The JSON that configures a `BatchedSim`: the text of each TOML config plus the mode
//! and stage to run. Python reads the files (`ninelives.sim_config`) and passes their
//! text, so the Rust module never touches the file system.

use nl_sim::modes::ModesConfig;
use nl_sim::reward::RewardConfig;
use nl_sim::scenarios::{ScenariosConfig, Stage};
use nl_sim::training::TrainingConfig;
use nl_sim::vehicle_config::VehicleConfig;
use serde::Deserialize;

/// The fields of the config JSON. Every field is required.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimConfigJson {
    vehicle_toml: String,
    modes_toml: String,
    scenarios_toml: String,
    reward_toml: String,
    /// "realistic" or "arcade".
    mode: String,
    /// "A" or "B".
    stage: String,
    /// configs/controllers.toml (PD's gains, for `run_eval`).
    controllers_toml: String,
    /// Domain randomization (SPEC §9.8) for `BatchedSim` episodes and, with `eval --dr`,
    /// for `run_eval`.
    randomize: bool,
}

/// A parsed config: what to build each environment from, and which stage it starts in.
pub struct SimConfig {
    /// The car, mode, scenarios and reward every environment is built from.
    pub training: TrainingConfig,
    /// The stage every environment starts in.
    pub stage: Stage,
    /// The text of controllers.toml (for PD in `run_eval`).
    pub controllers_toml: String,
}

/// Parses the config JSON. The error says which part is wrong.
pub fn parse_sim_config(json: &str) -> Result<SimConfig, String> {
    let fields: SimConfigJson =
        serde_json::from_str(json).map_err(|error| format!("sim config JSON: {error}"))?;
    let vehicle = VehicleConfig::from_toml_str(&fields.vehicle_toml).map_err(as_message)?;
    let modes = ModesConfig::from_toml_str(&fields.modes_toml).map_err(as_message)?;
    let scenarios = ScenariosConfig::from_toml_str(&fields.scenarios_toml).map_err(as_message)?;
    let reward = RewardConfig::from_toml_str(&fields.reward_toml).map_err(as_message)?;
    let mode = modes.by_name(&fields.mode).ok_or_else(|| {
        format!(
            "sim config JSON: mode must be \"realistic\" or \"arcade\", not {:?}",
            fields.mode
        )
    })?;
    let stage = parse_stage(&fields.stage)?;
    Ok(SimConfig {
        training: TrainingConfig {
            vehicle,
            mode,
            scenarios,
            reward,
            randomize: fields.randomize,
        },
        stage,
        controllers_toml: fields.controllers_toml,
    })
}

/// A config error as the message Python shows (each names its file and field).
fn as_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// "A" or "B" as a `Stage`, or an error that lists the valid names.
pub fn parse_stage(name: &str) -> Result<Stage, String> {
    Stage::from_name(name).ok_or_else(|| format!("stage must be \"A\" or \"B\", not {name:?}"))
}
