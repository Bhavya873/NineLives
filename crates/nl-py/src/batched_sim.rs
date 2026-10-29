//! `BatchedSim` (SPEC §9.1, ADR-002): N training environments stepped together with one
//! call from Python. The worlds step in parallel with rayon while the GIL is released;
//! everything crosses the boundary as numpy arrays (float32 observations, N × 43).

use nl_control::{ControllerChoices, HybridSwitch, RESIDUAL_ACTION_COUNT, ResidualPd};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::obs::{OBS_DIM, Observation};
use nl_sim::reward::RewardTerms;
use nl_sim::scenarios::ScenariosConfig;
use nl_sim::training::{FinishedEpisode, StageChoice};
use numpy::ndarray::Array2;
use numpy::{IntoPyArray, PyArray1, PyArray2, PyReadonlyArray1, PyReadonlyArray2};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use rayon::prelude::*;

use crate::config::{SimConfig, parse_sim_config, parse_stage};
use crate::env_slot::{EnvSlot, SlotControl, SlotStep};
use crate::hybrid_pd::HybridPd;

/// What `step` returns: observations, rewards, terminated, truncated, info.
type StepReturn<'py> = (
    Bound<'py, PyArray2<f32>>,
    Bound<'py, PyArray1<f32>>,
    Bound<'py, PyArray1<bool>>,
    Bound<'py, PyArray1<bool>>,
    Bound<'py, PyDict>,
);

/// N independent training environments, stepped as one.
#[pyclass(module = "ninelives_sim")]
pub struct BatchedSim {
    slots: Vec<EnvSlot>,
    /// Actions per environment: one per nozzle, plus the gate for gated residual control.
    action_size: usize,
    /// The scenarios every slot was built with (for `level_count`).
    scenarios: ScenariosConfig,
}

#[pymethods]
impl BatchedSim {
    /// `n_envs` environments built from `config_json` (see `ninelives.sim_config`);
    /// `seed` seeds every environment's own stream of episode seeds.
    #[new]
    fn new(n_envs: usize, config_json: &str, seed: u64) -> PyResult<BatchedSim> {
        if n_envs == 0 {
            return Err(PyValueError::new_err(
                "BatchedSim: n_envs must be at least 1",
            ));
        }
        let config = parse_sim_config(config_json).map_err(PyValueError::new_err)?;
        let control = slot_control(&config)?;
        let gated = config.residual.is_some_and(|settings| settings.gated);
        let action_size = if gated {
            RESIDUAL_ACTION_COUNT
        } else {
            NOZZLE_COUNT
        };
        let mut slots = Vec::with_capacity(n_envs);
        for index in 0..n_envs {
            let training = config.training.clone();
            slots.push(EnvSlot::new(
                training,
                config.stage,
                seed,
                index as u64,
                control.clone(),
            ));
        }
        Ok(BatchedSim {
            slots,
            action_size,
            scenarios: config.training.scenarios,
        })
    }

    /// Actions per environment `step` expects: 10 (one per nozzle), or 11 with gated
    /// residual control (the last is the gate).
    fn action_size(&self) -> usize {
        self.action_size
    }

    /// How many environments there are.
    #[getter]
    fn num_envs(&self) -> usize {
        self.slots.len()
    }

    /// Starts a new episode in every environment and returns the first observations
    /// (N × 43). `seeds` (N unsigned integers) picks each episode; without it each
    /// environment takes the next seed of its own stream.
    #[pyo3(signature = (seeds=None))]
    fn reset<'py>(
        &mut self,
        py: Python<'py>,
        seeds: Option<PyReadonlyArray1<'py, u64>>,
    ) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let seeds = self.seeds_for_reset(seeds)?;
        let mut observations = Vec::with_capacity(self.slots.len());
        for (slot, seed) in self.slots.iter_mut().zip(seeds) {
            observations.push(slot.reset(seed).map_err(PyValueError::new_err)?);
        }
        observations_array(py, &observations)
    }

    /// One control step for every environment. `actions` is N × 10 float32 in −1..1
    /// (SPEC §9.3). Ended environments start their next episode at once; their last
    /// observation is in `info["terminal_obs"]` (rows where `info["done"]` is true).
    /// After an error some environments may have stepped and others not: call
    /// `reset()` before stepping again.
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        actions: PyReadonlyArray2<'py, f32>,
    ) -> PyResult<StepReturn<'py>> {
        let actions = self.action_rows(&actions)?;
        let slots = &mut self.slots;
        let steps: Result<Vec<SlotStep>, String> = py.detach(|| {
            slots
                .par_iter_mut()
                .zip(actions.par_iter())
                .map(|(slot, action)| slot.step(*action))
                .collect()
        });
        step_return(py, &steps.map_err(PyValueError::new_err)?)
    }

    /// Restarts every environment's stream of episode seeds from `seed` (the same
    /// streams a new `BatchedSim` with this seed would have). Takes effect at each
    /// environment's next episode.
    fn reseed(&mut self, seed: u64) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            slot.reseed(seed, index as u64);
        }
    }

    /// How many curriculum levels `stage` ("A" to "F") has; the last is the full stage.
    fn level_count(&self, stage: &str) -> PyResult<u32> {
        let stage = parse_stage(stage).map_err(PyValueError::new_err)?;
        Ok(self.scenarios.level_count(stage))
    }

    /// Sets the stage ("A" to "F") and curriculum level for every environment's next
    /// episode (SPEC §9.9); running episodes finish as they started.
    fn set_stage_and_level(&mut self, stage: &str, level: u32) -> PyResult<()> {
        let stage = parse_stage(stage).map_err(PyValueError::new_err)?;
        for slot in &mut self.slots {
            slot.set_stage_and_level(stage, level);
        }
        Ok(())
    }

    /// Sets the stages every environment's next episodes pick from (SPEC §9.9): a list
    /// of `(stage, weight, level)`. Each episode draws its stage by weight from its own
    /// seed in Rust, so a seed always gets the same stage.
    fn set_stage_mix(&mut self, mix: Vec<(String, f32, u32)>) -> PyResult<()> {
        let mut choices = Vec::with_capacity(mix.len());
        for (name, weight, level) in mix {
            let stage = parse_stage(&name).map_err(PyValueError::new_err)?;
            choices.push(StageChoice {
                stage,
                weight,
                level,
            });
        }
        for slot in &mut self.slots {
            slot.set_stage_mix(choices.clone())
                .map_err(|error| PyValueError::new_err(format!("BatchedSim.{error}")))?;
        }
        Ok(())
    }
}

impl BatchedSim {
    /// One optional seed per environment, checking the count.
    fn seeds_for_reset(
        &self,
        seeds: Option<PyReadonlyArray1<'_, u64>>,
    ) -> PyResult<Vec<Option<u64>>> {
        let count = self.slots.len();
        let Some(seeds) = seeds else {
            return Ok(vec![None; count]);
        };
        let seeds = seeds.as_array();
        if seeds.len() != count {
            let message = format!(
                "BatchedSim.reset: got {} seeds for {count} envs",
                seeds.len()
            );
            return Err(PyValueError::new_err(message));
        }
        Ok(seeds.iter().map(|seed| Some(*seed)).collect())
    }

    /// The actions as one row per environment, checking the shape. A row without a gate
    /// gets a fully open one, which only gated residual control reads.
    fn action_rows(
        &self,
        actions: &PyReadonlyArray2<'_, f32>,
    ) -> PyResult<Vec<[f32; RESIDUAL_ACTION_COUNT]>> {
        let actions = actions.as_array();
        let expected = (self.slots.len(), self.action_size);
        if actions.dim() != expected {
            let message = format!(
                "BatchedSim.step: actions must have shape {expected:?}, not {:?}",
                actions.dim()
            );
            return Err(PyValueError::new_err(message));
        }
        let mut rows = Vec::with_capacity(expected.0);
        for row in actions.rows() {
            let mut action = [0.0; RESIDUAL_ACTION_COUNT];
            action[NOZZLE_COUNT] = 1.0;
            for (index, value) in row.iter().enumerate() {
                action[index] = *value;
            }
            rows.push(action);
        }
        Ok(rows)
    }
}

/// PD for the config's car with the config's residual scale, if residual control is on
/// (one copy per environment, so each keeps its own warm start). Like eval, PD is built
/// How each environment applies the policy's action. PD (for residual control or behind
/// the Hybrid gate) is built as the game and the eval runners build it: once, for the
/// config's base car and mode, not each episode's randomized one.
fn slot_control(config: &SimConfig) -> PyResult<SlotControl> {
    if config.residual.is_none() && !config.hybrid {
        return Ok(SlotControl::Direct);
    }
    let training = &config.training;
    let choices = ControllerChoices::from_configs(
        &training.vehicle,
        training.mode,
        &config.controllers_toml,
        None,
    )
    .map_err(|error| PyValueError::new_err(format!("BatchedSim: {error}")))?;
    let Some(settings) = config.residual else {
        let switch = HybridSwitch::new(choices.pd, choices.hybrid_gate);
        return Ok(SlotControl::Hybrid(HybridPd::new(switch)));
    };
    let residual = ResidualPd::new(choices.pd, settings)
        .map_err(|error| PyValueError::new_err(format!("BatchedSim: {error}")))?;
    Ok(SlotControl::Residual(residual))
}

/// Packs the per-environment results into SB3's (obs, reward, terminated, truncated,
/// info) shape.
fn step_return<'py>(py: Python<'py>, steps: &[SlotStep]) -> PyResult<StepReturn<'py>> {
    let mut observations = Vec::with_capacity(steps.len());
    let mut rewards = Vec::with_capacity(steps.len());
    let mut terminated = Vec::with_capacity(steps.len());
    let mut truncated = Vec::with_capacity(steps.len());
    for step in steps {
        observations.push(step.observation);
        rewards.push(step.terms.total());
        terminated.push(step.terminated);
        truncated.push(step.truncated);
    }
    Ok((
        observations_array(py, &observations)?,
        rewards.into_pyarray(py),
        terminated.into_pyarray(py),
        truncated.into_pyarray(py),
        info_dict(py, steps)?,
    ))
}

/// SPEC §9.1 `info`, one row per environment:
/// - `done`: the row's episode (for the policy) ended this step (bool);
/// - `terminal_obs`: that episode's last observation (N × 43; zeros where not done);
/// - `episode`: the row of a sim episode that ended as a JSON object string
///   (`FinishedEpisode`: the eval `EpisodeResult` fields plus `stage`, `level`,
///   `control_steps`), else None. Without Hybrid gating it comes exactly with `done`;
///   with gating an episode can end under PD after the policy's last stretch;
/// - `reward_terms`: this step's reward per term (N × `reward_term_names().len()`, in that order).
fn info_dict<'py>(py: Python<'py>, steps: &[SlotStep]) -> PyResult<Bound<'py, PyDict>> {
    let count = steps.len();
    let mut done = vec![false; count];
    let mut terminal_obs = vec![[0.0; OBS_DIM]; count];
    let mut episodes: Vec<Option<String>> = vec![None; count];
    for (row, step) in steps.iter().enumerate() {
        if let Some(last_observation) = step.terminal_observation {
            done[row] = true;
            terminal_obs[row] = last_observation;
        }
        if let Some(episode) = &step.finished {
            episodes[row] = Some(episode_json(episode)?);
        }
    }
    let info = PyDict::new(py);
    info.set_item("done", done.into_pyarray(py))?;
    info.set_item("terminal_obs", observations_array(py, &terminal_obs)?)?;
    info.set_item("episode", episodes)?;
    info.set_item("reward_terms", reward_terms_array(py, steps)?)?;
    Ok(info)
}

/// A finished episode as one flat JSON object, with the same field names as the
/// results files.
fn episode_json(episode: &FinishedEpisode) -> PyResult<String> {
    serde_json::to_string(episode)
        .map_err(|error| PyValueError::new_err(format!("BatchedSim: episode row: {error}")))
}

/// Observations as an N × 43 float32 array.
fn observations_array<'py>(
    py: Python<'py>,
    observations: &[Observation],
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let mut flat = Vec::with_capacity(observations.len() * OBS_DIM);
    for observation in observations {
        flat.extend_from_slice(observation);
    }
    table(py, observations.len(), OBS_DIM, flat)
}

/// Each step's reward terms as an N × `RewardTerms::NAMES.len()` float32 array (columns: `RewardTerms::NAMES`).
fn reward_terms_array<'py>(
    py: Python<'py>,
    steps: &[SlotStep],
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let mut flat = Vec::with_capacity(steps.len() * RewardTerms::NAMES.len());
    for step in steps {
        flat.extend_from_slice(&step.terms.as_array());
    }
    table(py, steps.len(), RewardTerms::NAMES.len(), flat)
}

/// A rows × columns float32 array from row-major values.
fn table(
    py: Python<'_>,
    rows: usize,
    columns: usize,
    values: Vec<f32>,
) -> PyResult<Bound<'_, PyArray2<f32>>> {
    let array = Array2::from_shape_vec((rows, columns), values)
        .map_err(|error| PyValueError::new_err(format!("BatchedSim: {error}")))?;
    Ok(array.into_pyarray(py))
}
