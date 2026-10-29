//! Policy runtime tests (SPEC §12 Policy): the loader rejects a wrong hash or shape, and
//! the forward pass matches a small network worked out by hand.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_policy::{POLICY_FORMAT, POLICY_FORMAT_VERSION, Policy, PolicyError};
use nl_sim::obs::{OBS_DIM, obs_schema_hash};
use serde_json::{Value, json};

/// An `OBS_DIM` → 2 (tanh) → 10 network: hidden unit 0 reads obs[0], hidden unit 1 reads
/// 0.5 × obs[1] + 0.1; every action is hidden0 − hidden1 + `action_bias`. No scaling
/// in the normalizer (mean 0, var 1, eps 0), clip at 10.
fn small_policy(action_bias: f32) -> Value {
    let mut hidden_weights = vec![0.0_f32; 2 * OBS_DIM];
    hidden_weights[0] = 1.0;
    hidden_weights[OBS_DIM + 1] = 0.5;
    let mut action_weights = Vec::new();
    for _ in 0..10 {
        action_weights.extend([1.0_f32, -1.0]);
    }
    json!({
        "format": POLICY_FORMAT, "version": POLICY_FORMAT_VERSION,
        "obs_schema_hash": obs_schema_hash(), "obs_dim": OBS_DIM, "act_dim": 10,
        "obs_norm": { "mean": vec![0.0; OBS_DIM], "var": vec![1.0; OBS_DIM], "eps": 0.0, "clip": 10.0 },
        "layers": [
            { "w": hidden_weights, "b": [0.0, 0.1], "in": OBS_DIM, "out": 2, "act": "tanh" },
            { "w": action_weights, "b": vec![action_bias; 10], "in": 2, "out": 10, "act": "none" },
        ],
        "action": { "clip": [-1, 1], "map": "(a+1)/2" },
        "trained": { "run_id": "test", "steps": 0, "git_sha": "none" },
    })
}

fn load(file: &Value) -> Result<Policy, PolicyError> {
    Policy::from_json(&file.to_string())
}

#[test]
fn the_forward_pass_matches_a_hand_calculation() {
    let policy = load(&small_policy(0.0)).unwrap();
    let mut observation = [0.0; OBS_DIM];
    observation[0] = 0.5;
    observation[1] = 2.0;

    let action = policy.act(&observation);

    // tanh(0.5) − tanh(0.5 × 2 + 0.1) = 0.46212 − 0.80050
    let expected = 0.5_f32.tanh() - 1.1_f32.tanh();
    for value in action {
        assert!((value - expected).abs() < 1e-6, "{value} vs {expected}");
    }
}

#[test]
fn observations_are_clipped_after_normalizing_and_actions_to_minus_one_to_one() {
    let policy = load(&small_policy(5.0)).unwrap();
    let mut observation = [0.0; OBS_DIM];
    observation[0] = 1000.0;

    let action = policy.act(&observation);

    assert_eq!(action, [1.0; 10]);
}

#[test]
fn a_policy_for_another_observation_layout_is_refused() {
    let mut file = small_policy(0.0);
    file["obs_schema_hash"] = json!("0000000000000000");

    let error = load(&file).unwrap_err();

    assert!(matches!(error, PolicyError::WrongSchema { .. }));
    assert!(error.to_string().contains(&obs_schema_hash()));
}

#[test]
fn a_file_of_another_format_is_refused() {
    let mut file = small_policy(0.0);
    file["format"] = json!("something-else");

    assert!(matches!(
        load(&file).unwrap_err(),
        PolicyError::WrongFormat { .. }
    ));
}

#[test]
fn layers_that_do_not_chain_are_refused_by_number() {
    let mut file = small_policy(0.0);
    file["layers"][1]["in"] = json!(3);

    let error = load(&file).unwrap_err();

    assert!(error.to_string().contains("layer 1"), "{error}");
}

#[test]
fn weights_of_the_wrong_length_are_refused() {
    let mut file = small_policy(0.0);
    file["layers"][0]["b"] = json!([0.0]);

    assert!(matches!(load(&file).unwrap_err(), PolicyError::BadShape(_)));
}

#[test]
fn a_negative_clip_or_a_zero_scale_is_refused() {
    let mut negative_clip = small_policy(0.0);
    negative_clip["obs_norm"]["clip"] = json!(-1.0);
    let mut zero_scale = small_policy(0.0);
    zero_scale["obs_norm"]["var"][3] = json!(0.0);

    assert!(matches!(
        load(&negative_clip).unwrap_err(),
        PolicyError::BadShape(_)
    ));
    let error = load(&zero_scale).unwrap_err();
    assert!(error.to_string().contains("value 3"), "{error}");
}
