"""Hybrid-gated training (DECISIONS 2026-10-21): PD flies hidden until the game's gate
hands the car to the AI, and each AI stretch is one episode for the policy. The policy is
only ever handed an upset car, the gate's release ends its episode while the sim episode's
row still arrives, the same seed replays the same stretches, and bad configs are refused."""

import json

import numpy as np
import pytest

from ninelives.sim_config import REPO_ROOT, load_sim_config
from ninelives_sim import BatchedSim, policy_actions

# vehicle.toml [events] upset_tilt_deg: the gate hands over above this.
UPSET_TILT_DEG = 60.0
SHIPPED_POLICY = (REPO_ROOT / "models" / "policy.json").read_text()
NO_THRUST = -1.0  # an action of −1 is a closed valve


def gated_sim(stage: str, n_envs: int = 4, seed: int = 0) -> BatchedSim:
    return BatchedSim(n_envs, load_sim_config("realistic", stage, controller="hybrid"), seed)


def tilt_deg(observations: np.ndarray) -> np.ndarray:
    """Tilt from upright of each observation (obs 1 is world-down's body Y: −1 upright)."""
    up_alignment = np.clip(-observations[:, 1], -1.0, 1.0)
    return np.degrees(np.arccos(up_alignment))


def test_every_policy_episode_starts_with_a_car_the_gate_called_upset():
    sim = gated_sim("F")
    first_observations = [sim.reset()]
    actions = np.full((4, 10), NO_THRUST, dtype=np.float32)

    for _ in range(600):
        observations, _, _, _, info = sim.step(actions)
        first_observations.append(observations[info["done"]])

    starts = np.concatenate(first_observations)
    assert len(starts) > 4, "the AI should crash and get new flights"
    assert np.all(tilt_deg(starts) > UPSET_TILT_DEG)


def test_a_saved_flight_ends_at_the_gates_release_and_its_row_still_arrives():
    sim = gated_sim("A", n_envs=8, seed=3)
    # Level 0's small tilts never reach the gate; the hardest level is the real stage.
    sim.set_stage_and_level("A", sim.level_count("A") - 1)
    observations = sim.reset()
    released_rows = []

    for _ in range(1500):
        actions = policy_actions(SHIPPED_POLICY, observations)
        observations, _, terminated, truncated, info = sim.step(actions)
        for row in np.flatnonzero(info["done"] & terminated & ~truncated):
            episode = info["episode"][row]
            if episode is not None and json.loads(episode)["success"]:
                released_rows.append(json.loads(episode))

    # The gate releases a settled car before Stage A's 1 s hold is up, so a saved
    # flight's success row comes in on the release step, after PD finished the hold.
    assert released_rows, "the shipped policy saves some Stage A flights"
    assert all(row["stage"] == "A" for row in released_rows)


def test_the_same_seed_replays_the_same_flights():
    first, second = gated_sim("F", seed=7), gated_sim("F", seed=7)
    rng = np.random.default_rng(0)
    first_obs, second_obs = first.reset(), second.reset()
    assert np.array_equal(first_obs, second_obs)

    for _ in range(300):
        actions = rng.uniform(-1.0, 1.0, size=(4, 10)).astype(np.float32)
        first_step, second_step = first.step(actions), second.step(actions)

        for first_part, second_part in zip(first_step[:4], second_step[:4], strict=True):
            assert np.array_equal(first_part, second_part)


def test_hybrid_gating_with_residual_control_is_refused():
    config = json.loads(load_sim_config("realistic", "A", controller="hybrid"))
    config["residual"] = {"base": "pd", "scale": 0.5}

    with pytest.raises(ValueError, match="cannot be used with residual"):
        BatchedSim(1, json.dumps(config), 0)


def test_an_unknown_controller_name_is_refused():
    with pytest.raises(ValueError, match='controller must be "hybrid"'):
        BatchedSim(1, load_sim_config("realistic", "A", controller="pd"), 0)
