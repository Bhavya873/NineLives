//! The policy file (SPEC §9.12) and the forward pass: normalize the observation with the
//! training run's VecNormalize statistics, then run each dense layer
//! (`y = act(W·x + b)`, W row-major `out × in`), then clip the actions to [−1, 1].

use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::obs::{OBS_DIM, Observation, obs_schema_hash};
use serde::Deserialize;

use crate::{POLICY_FORMAT, POLICY_FORMAT_VERSION};

/// Why a policy file was refused.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// Not valid JSON, or a field is missing or has the wrong type.
    #[error("policy.json: {0}")]
    Json(#[from] serde_json::Error),

    /// The file is not a Nine Lives policy, or a version this build cannot read.
    #[error(
        "policy.json: format must be \"{POLICY_FORMAT}\" version {POLICY_FORMAT_VERSION}, but it is \"{format}\" version {version}"
    )]
    WrongFormat { format: String, version: u32 },

    /// The policy was trained on a different observation layout (CLAUDE.md rule 4).
    #[error(
        "policy.json: obs_schema_hash is {found}, but this build's observation layout is {expected}; re-export the policy from a run trained on this build"
    )]
    WrongSchema { found: String, expected: String },

    /// A size in the file does not fit the network it describes.
    #[error("policy.json: {0}")]
    BadShape(String),
}

/// The JSON file as written by `ninelives.export` (SPEC §9.12).
#[derive(Deserialize)]
struct PolicyFile {
    format: String,
    version: u32,
    obs_schema_hash: String,
    obs_dim: usize,
    act_dim: usize,
    obs_norm: ObsNorm,
    layers: Vec<LayerFile>,
}

/// VecNormalize's statistics: `x' = clip((x − mean) / sqrt(var + eps), ±clip)`.
#[derive(Deserialize)]
struct ObsNorm {
    mean: Vec<f32>,
    var: Vec<f32>,
    eps: f32,
    clip: f32,
}

/// One dense layer as stored in the file.
#[derive(Deserialize)]
struct LayerFile {
    w: Vec<f32>,
    b: Vec<f32>,
    #[serde(rename = "in")]
    inputs: usize,
    #[serde(rename = "out")]
    outputs: usize,
    act: String,
}

/// One dense layer, checked and ready to run.
#[derive(Debug, Clone, PartialEq)]
struct Layer {
    /// Row-major `outputs × inputs`.
    weights: Vec<f32>,
    biases: Vec<f32>,
    inputs: usize,
    outputs: usize,
    /// tanh after the sum when true; nothing otherwise.
    tanh: bool,
}

/// A loaded policy: observation statistics plus the MLP.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    obs_mean: Vec<f32>,
    /// `sqrt(var + eps)` per value, worked out once when the file is loaded.
    obs_scale: Vec<f32>,
    obs_clip: f32,
    layers: Vec<Layer>,
}

impl Policy {
    /// Reads and checks a policy file: format, schema hash and every size.
    pub fn from_json(text: &str) -> Result<Policy, PolicyError> {
        let file: PolicyFile = serde_json::from_str(text)?;
        check_header(&file)?;
        let norm = &file.obs_norm;
        check_obs_norm(norm)?;
        let layers = build_layers(&file.layers)?;
        let mut obs_scale = Vec::with_capacity(OBS_DIM);
        for var in &norm.var {
            obs_scale.push((var + norm.eps).sqrt());
        }
        Ok(Policy {
            obs_mean: norm.mean.clone(),
            obs_scale,
            obs_clip: norm.clip,
            layers,
        })
    }

    /// The deterministic (mean) action for `observation`, each in −1..1 (SPEC §9.3).
    pub fn act(&self, observation: &Observation) -> [f32; NOZZLE_COUNT] {
        let mut values = self.normalize(observation);
        for layer in &self.layers {
            values = layer.forward(&values);
        }
        let mut action = [0.0; NOZZLE_COUNT];
        for (index, value) in values.iter().take(NOZZLE_COUNT).enumerate() {
            action[index] = value.clamp(-1.0, 1.0);
        }
        action
    }

    /// SPEC §9.12 obs_norm: the same transform VecNormalize applied during training.
    fn normalize(&self, observation: &Observation) -> Vec<f32> {
        let mut normalized = Vec::with_capacity(OBS_DIM);
        for (index, raw) in observation.iter().enumerate() {
            let value = (raw - self.obs_mean[index]) / self.obs_scale[index];
            normalized.push(value.clamp(-self.obs_clip, self.obs_clip));
        }
        normalized
    }
}

impl Layer {
    /// `act(W·x + b)`.
    fn forward(&self, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(self.outputs);
        for row in 0..self.outputs {
            let weights = &self.weights[row * self.inputs..(row + 1) * self.inputs];
            let mut sum = self.biases[row];
            for (weight, value) in weights.iter().zip(input) {
                sum += weight * value;
            }
            // libm: the browser and native must compute the same actions (rule 2).
            output.push(if self.tanh { libm::tanhf(sum) } else { sum });
        }
        output
    }
}

/// Format, version, schema hash and the two outer sizes.
fn check_header(file: &PolicyFile) -> Result<(), PolicyError> {
    if file.format != POLICY_FORMAT || file.version != POLICY_FORMAT_VERSION {
        return Err(PolicyError::WrongFormat {
            format: file.format.clone(),
            version: file.version,
        });
    }
    let expected = obs_schema_hash();
    if file.obs_schema_hash != expected {
        return Err(PolicyError::WrongSchema {
            found: file.obs_schema_hash.clone(),
            expected,
        });
    }
    if file.obs_dim != OBS_DIM || file.act_dim != NOZZLE_COUNT {
        return Err(shape(&format!(
            "obs_dim and act_dim must be {OBS_DIM} and {NOZZLE_COUNT}, not {} and {}",
            file.obs_dim, file.act_dim
        )));
    }
    Ok(())
}

/// The normalizer must give a finite, positive scale for every value and a clip of
/// zero or more (a negative clip would make `clamp` panic).
fn check_obs_norm(norm: &ObsNorm) -> Result<(), PolicyError> {
    if norm.mean.len() != OBS_DIM || norm.var.len() != OBS_DIM {
        return Err(shape(
            "obs_norm.mean and obs_norm.var must each have obs_dim values",
        ));
    }
    if !norm.clip.is_finite() || norm.clip < 0.0 {
        return Err(shape("obs_norm.clip must be a finite number, zero or more"));
    }
    for (index, (mean, var)) in norm.mean.iter().zip(&norm.var).enumerate() {
        let scale_squared = var + norm.eps;
        if !mean.is_finite() || !scale_squared.is_finite() || scale_squared <= 0.0 {
            let message =
                format!("obs_norm value {index}: mean must be finite and var + eps above zero");
            return Err(shape(&message));
        }
    }
    Ok(())
}

/// Checks that the layers chain from obs_dim to act_dim with consistent sizes.
fn build_layers(files: &[LayerFile]) -> Result<Vec<Layer>, PolicyError> {
    if files.is_empty() {
        return Err(shape("layers is empty"));
    }
    let mut layers = Vec::with_capacity(files.len());
    let mut expected_inputs = OBS_DIM;
    for (number, file) in files.iter().enumerate() {
        if file.inputs != expected_inputs {
            let message = format!(
                "layer {number} takes {} inputs, but gets {expected_inputs}",
                file.inputs
            );
            return Err(shape(&message));
        }
        if file.w.len() != file.inputs * file.outputs || file.b.len() != file.outputs {
            let message =
                format!("layer {number}: w must have in × out values and b must have out");
            return Err(shape(&message));
        }
        let tanh = match file.act.as_str() {
            "tanh" => true,
            "none" => false,
            other => {
                return Err(shape(&format!(
                    "layer {number}: act must be \"tanh\" or \"none\", not \"{other}\""
                )));
            }
        };
        layers.push(Layer {
            weights: file.w.clone(),
            biases: file.b.clone(),
            inputs: file.inputs,
            outputs: file.outputs,
            tanh,
        });
        expected_inputs = file.outputs;
    }
    if expected_inputs != NOZZLE_COUNT {
        return Err(shape(&format!(
            "the last layer must give {NOZZLE_COUNT} outputs, not {expected_inputs}"
        )));
    }
    Ok(layers)
}

fn shape(message: &str) -> PolicyError {
    PolicyError::BadShape(message.to_string())
}
