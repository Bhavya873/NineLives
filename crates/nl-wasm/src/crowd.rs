//! The training viewer's crowd (SPEC §11.6, M6): many Stage A drops at once, all flown by
//! one policy (one checkpoint), for the browser to draw side by side. Each car is a full
//! `nl_sim::episode::Episode` stepped by `nl_control::eval::step_episode`, so landings,
//! crashes and time limits follow the same rules as training and eval; the policy is
//! parsed once and shared by every car's `Ai`.

use std::sync::Arc;

use nl_control::Ai;
use nl_control::eval::step_episode;
use nl_policy::Policy;
use nl_sim::conventions::{NOZZLE_COUNT, WHEEL_COUNT};
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use wasm_bindgen::prelude::*;

use crate::{FLOATS_PER_POSE, flatten_poses};

/// What `outcomes` reports for each car.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum CrowdOutcome {
    /// Still in the air or settling.
    Running = 0,
    /// Landed and held all four wheels down (SPEC §9.5).
    Landed = 1,
    /// Crashed.
    Crashed = 2,
    /// Ran out of time.
    TimedOut = 3,
}

/// A crowd of Stage A drops flown by one policy.
#[wasm_bindgen]
pub struct Crowd {
    episodes: Vec<Episode>,
    /// Each car's AI, all sharing one policy.
    pilots: Vec<Ai>,
    /// Each car's throttles, held between control steps (SPEC §7.1).
    throttles: Vec<[f32; NOZZLE_COUNT]>,
}

#[wasm_bindgen]
impl Crowd {
    /// `count` cars, car `i` dropped from Stage A seed `first_seed + i` (full difficulty,
    /// no randomization) in `mode` ("realistic" or "arcade"), all flown by the policy in
    /// `policy_json` (a checkpoint file or models/policy.json).
    #[wasm_bindgen(constructor)]
    pub fn new(
        policy_json: &str,
        mode: &str,
        first_seed: u64,
        count: u32,
    ) -> Result<Crowd, JsError> {
        let policy = Arc::new(Policy::from_json(policy_json)?);
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?
            .by_name(mode)
            .ok_or_else(|| JsError::new("Crowd: mode must be realistic or arcade"))?;
        let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?;
        let plain = EpisodeVariation::none(&vehicle, mode);
        let mut episodes = Vec::new();
        for offset in 0..u64::from(count) {
            let seed = first_seed
                .checked_add(offset)
                .ok_or_else(|| JsError::new("Crowd: first_seed is too large"))?;
            episodes.push(Episode::start(&plain, &scenarios, Stage::A, seed)?);
        }
        let pilots = vec![Ai::new(policy); episodes.len()];
        let throttles = vec![[0.0; NOZZLE_COUNT]; episodes.len()];
        Ok(Crowd {
            episodes,
            pilots,
            throttles,
        })
    }

    /// How many cars there are.
    pub fn count(&self) -> usize {
        self.episodes.len()
    }

    /// One physics step (1/120 s) for every car still running, the policy acting at
    /// 60 Hz exactly as in eval. Returns false once every car's episode has ended.
    pub fn step(&mut self) -> bool {
        // A finished episode's step does nothing, so every car can simply be stepped.
        let mut any_running = false;
        for index in 0..self.episodes.len() {
            let episode = &mut self.episodes[index];
            let status = step_episode(episode, &mut self.pilots[index], &mut self.throttles[index]);
            any_running |= status == EpisodeStatus::Running;
        }
        any_running
    }

    /// Every car's body pose, `count` × [x, y, z, qx, qy, qz, qw].
    pub fn chassis_poses(&self) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.episodes.len() * FLOATS_PER_POSE);
        for episode in &self.episodes {
            let pose = episode.world().chassis_pose();
            values.extend_from_slice(&pose.position_m);
            values.extend_from_slice(&pose.rotation_xyzw);
        }
        values
    }

    /// Every car's wheel poses, `count` × 4 × [x, y, z, qx, qy, qz, qw].
    pub fn wheel_poses(&self) -> Vec<f32> {
        let size = self.episodes.len() * WHEEL_COUNT * FLOATS_PER_POSE;
        let mut values = Vec::with_capacity(size);
        for episode in &self.episodes {
            values.extend(flatten_poses(&episode.world().wheel_poses()));
        }
        values
    }

    /// Every car's outcome so far, as `CrowdOutcome` numbers.
    pub fn outcomes(&self) -> Vec<u8> {
        let mut codes = Vec::with_capacity(self.episodes.len());
        for episode in &self.episodes {
            let outcome = match episode.status() {
                EpisodeStatus::Running => CrowdOutcome::Running,
                EpisodeStatus::Succeeded => CrowdOutcome::Landed,
                EpisodeStatus::Crashed(_) => CrowdOutcome::Crashed,
                EpisodeStatus::TimedOut => CrowdOutcome::TimedOut,
            };
            codes.push(outcome as u8);
        }
        codes
    }

    /// Every car's episode row so far (the eval `EpisodeResult` fields), as JSON.
    pub fn results_json(&self) -> Result<String, JsError> {
        let mut rows = Vec::with_capacity(self.episodes.len());
        for episode in &self.episodes {
            rows.push(episode.result());
        }
        Ok(serde_json::to_string(&rows)?)
    }
}
