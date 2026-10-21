//! Fixed-seed evaluation and the policy forward pass for Python (SPEC §9.12, §10).
//!
//! `run_eval` runs whole episodes with a Rust controller (Off, PD or the AI policy)
//! through `nl_control::eval::run_episode`, the same code as the Rust eval runner, so
//! Python's results match it exactly. `policy_actions` exposes the policy's forward pass
//! for the export parity test.

use nl_control::ControllerKind;
use nl_control::build_controller;
use nl_control::eval::{air_used_driving, run_episode, summarise};
use nl_policy::Policy;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::course::CourseSpec;
use nl_sim::episode::EpisodeResult;
use nl_sim::input::InputFrame;
use nl_sim::obs::{OBS_DIM, Observation};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};
use numpy::ndarray::Array2;
use numpy::{IntoPyArray, PyArray2, PyReadonlyArray2};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use rayon::prelude::*;

use crate::config::{SimConfig, parse_sim_config};

/// Runs `episodes` episodes with seeds `first_seed`, `first_seed + 1`, … using
/// `controller` ("off", "pd", "ai", "hybrid" or "manual"; "ai" and "hybrid" need `policy_json`). Episodes run in
/// parallel. Returns the report as JSON: `{"summary": …, "episodes": […]}`, the same
/// summary (SPEC §10 metrics) and per-episode rows the Rust eval runner writes.
#[pyfunction]
#[pyo3(signature = (controller, config_json, episodes, first_seed, policy_json=None))]
pub fn run_eval(
    py: Python<'_>,
    controller: &str,
    config_json: &str,
    episodes: u64,
    first_seed: u64,
    policy_json: Option<&str>,
) -> PyResult<String> {
    let config = parse_sim_config(config_json).map_err(PyValueError::new_err)?;
    let training = &config.training;
    let prototype = build_controller(
        controller,
        &training.vehicle,
        training.mode,
        &config.controllers_toml,
        policy_json,
    )
    .map_err(|error| PyValueError::new_err(format!("run_eval: {error}")))?;
    // Like the Rust runner: refuse seeds that would run past u64::MAX.
    if episodes > 0 && first_seed.checked_add(episodes - 1).is_none() {
        let message = "run_eval: first_seed is too large for that many episodes";
        return Err(PyValueError::new_err(message));
    }
    let results: Result<Vec<EpisodeResult>, String> = py.detach(|| {
        (0..episodes)
            .into_par_iter()
            // One controller per worker thread, reset by each episode: no copy of the
            // policy's weights per episode.
            .map_init(
                || prototype.clone(),
                |controller, offset| eval_one(&config, controller, first_seed + offset),
            )
            .collect()
    });
    let results = results.map_err(PyValueError::new_err)?;
    let report = serde_json::json!({ "summary": summarise(&results), "episodes": results });
    Ok(report.to_string())
}

/// Air used, in kg, by `controller` driving straight along `course_json` at half throttle
/// for `seconds`, with the config's car and mode and no episode rules: how much it fires
/// that the driver didn't need, e.g. over courses/bumps-strip.json (the PD tuner's check).
#[pyfunction]
#[pyo3(signature = (controller, config_json, course_json, seconds, policy_json=None))]
pub fn driving_air_kg(
    controller: &str,
    config_json: &str,
    course_json: &str,
    seconds: f32,
    policy_json: Option<&str>,
) -> PyResult<f32> {
    let config = parse_sim_config(config_json).map_err(PyValueError::new_err)?;
    let training = &config.training;
    let mut controller = build_controller(
        controller,
        &training.vehicle,
        training.mode,
        &config.controllers_toml,
        policy_json,
    )
    .map_err(|error| PyValueError::new_err(format!("driving_air_kg: {error}")))?;
    let course = CourseSpec::from_json_str(course_json)
        .map_err(|error| PyValueError::new_err(format!("driving_air_kg: course: {error}")))?;
    let mut world = World::new_on_course(
        &training.vehicle,
        training.mode,
        &course,
        DEFAULT_GROUND_FRICTION,
        0,
    );
    let half_throttle = InputFrame {
        throttle: 127,
        ..InputFrame::default()
    };
    Ok(air_used_driving(
        &mut world,
        &mut controller,
        half_throttle,
        seconds,
    ))
}

/// The policy's actions (each in −1..1) for N observations (N × 41 → N × 10).
#[pyfunction]
pub fn policy_actions<'py>(
    py: Python<'py>,
    policy_json: &str,
    observations: PyReadonlyArray2<'py, f32>,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let policy =
        Policy::from_json(policy_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let observations = observations.as_array();
    if observations.ncols() != OBS_DIM {
        let message = format!("policy_actions: observations need {OBS_DIM} columns");
        return Err(PyValueError::new_err(message));
    }
    let mut flat = Vec::with_capacity(observations.nrows() * NOZZLE_COUNT);
    for row in observations.rows() {
        let mut observation: Observation = [0.0; OBS_DIM];
        for (index, value) in row.iter().enumerate() {
            observation[index] = *value;
        }
        flat.extend(policy.act(&observation));
    }
    let actions = Array2::from_shape_vec((observations.nrows(), NOZZLE_COUNT), flat)
        .map_err(|error| PyValueError::new_err(format!("policy_actions: {error}")))?;
    Ok(actions.into_pyarray(py))
}

/// One episode from `seed` with `controller` (reset at the episode's start).
fn eval_one(
    config: &SimConfig,
    controller: &mut ControllerKind,
    seed: u64,
) -> Result<EpisodeResult, String> {
    let training = &config.training;
    // SPEC §10: eval leaves randomization off unless the config asks (`eval --dr`).
    let randomization = training
        .randomize
        .then_some(&training.scenarios.randomization);
    let variation =
        EpisodeVariation::for_seed(randomization, &training.vehicle, training.mode, seed);
    run_episode(
        &variation,
        &training.scenarios,
        config.stage,
        controller,
        seed,
    )
    .map_err(|error| format!("episode seed {seed}: {error}"))
}
