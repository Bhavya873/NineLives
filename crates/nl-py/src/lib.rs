//! The `ninelives_sim` Python module (SPEC §9.1): `BatchedSim` for training, `run_eval`
//! and `policy_actions` for evaluation and export checks, plus the observation layout
//! and its schema hash, so Python never re-declares them (CLAUDE.md golden rules 1, 4).

mod batched_sim;
mod config;
mod env_slot;
mod eval;
mod hybrid_pd;

use nl_sim::conventions;
use nl_sim::obs::{OBS_SPEC, obs_schema_hash};
use nl_sim::reward::RewardTerms;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::batched_sim::BatchedSim;
use crate::eval::{driving_air_kg, policy_actions, run_eval};

/// The nozzle names in their fixed index order.
#[pyfunction]
fn nozzle_names() -> Vec<&'static str> {
    conventions::NOZZLE_NAMES.to_vec()
}

/// The physics step length, in seconds.
#[pyfunction]
fn physics_dt_s() -> f32 {
    conventions::PHYSICS_DT_S
}

/// The observation layout (SPEC §9.2): one dict per value with its `name`, `index`,
/// `low` and `high`, in order.
#[pyfunction(name = "obs_spec")]
fn obs_spec_py(py: Python<'_>) -> PyResult<Vec<Bound<'_, PyDict>>> {
    let mut fields = Vec::with_capacity(OBS_SPEC.len());
    for (index, field) in OBS_SPEC.iter().enumerate() {
        let entry = PyDict::new(py);
        entry.set_item("name", field.name)?;
        entry.set_item("index", index)?;
        entry.set_item("low", field.low)?;
        entry.set_item("high", field.high)?;
        fields.push(entry);
    }
    Ok(fields)
}

/// The observation schema hash (16 hex digits); policies must carry the same one.
#[pyfunction(name = "obs_schema_hash")]
fn obs_schema_hash_py() -> String {
    obs_schema_hash()
}

/// A fingerprint (16 hex digits) of the given texts, in order; the same one the Rust eval
/// runner writes into its artifacts (`nl_sim::provenance::text_hash`).
#[pyfunction]
fn text_hash(parts: Vec<String>) -> String {
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    nl_sim::provenance::text_hash(&parts)
}

/// The fingerprint of the physics settings, from their file texts
/// (`nl_sim::provenance::physics_hash`).
#[pyfunction]
fn physics_hash(
    vehicle_toml: &str,
    modes_toml: &str,
    pieces_json: &str,
    approach_toml: &str,
) -> String {
    nl_sim::provenance::physics_hash(vehicle_toml, modes_toml, pieces_json, approach_toml)
}

/// The `format` and `version` an exported policy file must carry (SPEC §9.12).
#[pyfunction]
fn policy_format() -> (&'static str, u32) {
    (nl_policy::POLICY_FORMAT, nl_policy::POLICY_FORMAT_VERSION)
}

/// The names of `info["reward_terms"]` columns, in order.
#[pyfunction]
fn reward_term_names() -> Vec<&'static str> {
    RewardTerms::NAMES.to_vec()
}

/// The Python module definition. The function name must match the module name.
#[pymodule]
fn ninelives_sim(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<BatchedSim>()?;
    module.add_function(wrap_pyfunction!(run_eval, module)?)?;
    module.add_function(wrap_pyfunction!(driving_air_kg, module)?)?;
    module.add_function(wrap_pyfunction!(policy_actions, module)?)?;
    module.add_function(wrap_pyfunction!(policy_format, module)?)?;
    module.add_function(wrap_pyfunction!(nozzle_names, module)?)?;
    module.add_function(wrap_pyfunction!(physics_dt_s, module)?)?;
    module.add_function(wrap_pyfunction!(obs_spec_py, module)?)?;
    module.add_function(wrap_pyfunction!(obs_schema_hash_py, module)?)?;
    module.add_function(wrap_pyfunction!(reward_term_names, module)?)?;
    module.add_function(wrap_pyfunction!(text_hash, module)?)?;
    module.add_function(wrap_pyfunction!(physics_hash, module)?)?;
    Ok(())
}
