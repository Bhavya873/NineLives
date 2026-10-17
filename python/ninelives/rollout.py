"""Runs a trained policy on fixed seeds and collects how each episode ended.

Training uses it to keep the best checkpoint (SPEC §9.10): every episode starts from a
known seed, and the policy acts deterministically on observations normalized with the
run's VecNormalize statistics. Each result is BatchedSim's episode row (SPEC §10 fields).
"""

from typing import Any

import numpy as np
from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import VecNormalize

from ninelives.env import finished_episode
from ninelives.sim_config import load_sim_config
from ninelives_sim import BatchedSim


def evaluate_fixed_seeds(
    model: PPO,
    normalizer: VecNormalize,
    episodes: int,
    first_seed: int,
    mode: str,
    stage: str,
    level: int,
) -> list[dict[str, Any]]:
    """One episode per seed first_seed, first_seed + 1, …; their rows in seed order."""
    sim = BatchedSim(episodes, load_sim_config(mode, stage), first_seed)
    sim.set_stage_and_level(stage, level)
    observations = sim.reset(np.arange(first_seed, first_seed + episodes, dtype=np.uint64))
    rows: list[dict[str, Any] | None] = [None] * episodes
    while None in rows:
        actions, _ = model.predict(normalizer.normalize_obs(observations), deterministic=True)
        observations, _, _, _, info = sim.step(actions.astype(np.float32))
        for row in np.flatnonzero(info["done"]):
            if rows[row] is None:
                rows[row] = finished_episode(info, row)
    return rows


def success_and_crash_rates(rows: list[dict[str, Any]]) -> tuple[float, float]:
    """The fraction of episode rows that succeeded, and that crashed."""
    successes = sum(row["success"] for row in rows)
    crashes = sum(row["crash_cause"] is not None for row in rows)
    return successes / len(rows), crashes / len(rows)
