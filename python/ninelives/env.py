"""Gymnasium and Stable-Baselines3 adapters over the Rust `BatchedSim` (SPEC §9.1).

`NineLivesVecEnv` is what training uses: SB3's VecEnv interface over N worlds that step
in parallel inside Rust. `NineLivesEnv` is a single-env Gymnasium wrapper, used only for
`check_env` and debugging. Neither computes physics, rewards or episode rules.
"""

import json
from typing import Any

import gymnasium as gym
import numpy as np
from stable_baselines3.common.vec_env.base_vec_env import (
    VecEnv,
    VecEnvIndices,
    VecEnvObs,
    VecEnvStepReturn,
)

from ninelives.sim_config import load_sim_config
from ninelives_sim import BatchedSim, nozzle_names, obs_spec

ACTION_SIZE = len(nozzle_names())


def observation_space() -> gym.spaces.Box:
    """The observation Box, with bounds read from the Rust `ObsSpec`."""
    fields = obs_spec()
    low = np.array([field["low"] for field in fields], dtype=np.float32)
    high = np.array([field["high"] for field in fields], dtype=np.float32)
    return gym.spaces.Box(low=low, high=high, dtype=np.float32)


def action_space() -> gym.spaces.Box:
    """SPEC §9.3: one action per nozzle in [-1, 1]; the sim maps them to throttles."""
    return gym.spaces.Box(low=-1.0, high=1.0, shape=(ACTION_SIZE,), dtype=np.float32)


def finished_episode(info: dict[str, Any], row: int) -> dict[str, Any]:
    """The episode that ended in `row`: the eval row's fields (success, saves,
    crash_cause, air_used_kg, ...) plus stage, level and control_steps."""
    return json.loads(info["episode"][row])


class NineLivesVecEnv(VecEnv):
    """SB3 VecEnv over `BatchedSim`. Finished envs restart inside Rust; their last
    observation is passed on as `infos[i]["terminal_observation"]` and their row as
    `infos[i]["nl_episode"]`."""

    render_mode = None

    def __init__(
        self, n_envs: int, seed: int = 0, mode: str = "realistic", stage: str = "A"
    ) -> None:
        self.sim = BatchedSim(n_envs, load_sim_config(mode, stage), seed)
        super().__init__(n_envs, observation_space(), action_space())

    def seed(self, seed: int | None = None) -> list[int | None]:
        """Restarts every env's stream of episode seeds from `seed` (SB3 calls this with
        the PPO seed), so runs with different seeds see different episodes."""
        if seed is not None:
            self.sim.reseed(seed)
        return [seed for _ in range(self.num_envs)]

    def reset(self) -> VecEnvObs:
        """Starts a new episode in every env, each from the next seed of its stream."""
        self._reset_options()
        return self.sim.reset()

    def step_async(self, actions: np.ndarray) -> None:
        """Keeps the actions for `step_wait` (stepping happens there, in parallel)."""
        self._actions = np.asarray(actions, dtype=np.float32).reshape(self.num_envs, -1)

    def step_wait(self) -> VecEnvStepReturn:
        """One control step for every env."""
        observations, rewards, terminated, truncated, info = self.sim.step(self._actions)
        infos: list[dict[str, Any]] = [{} for _ in range(self.num_envs)]
        for row in np.flatnonzero(info["done"]):
            infos[row] = {
                "terminal_observation": info["terminal_obs"][row],
                "TimeLimit.truncated": bool(truncated[row]),
                "nl_episode": finished_episode(info, row),
            }
        return observations, rewards, terminated | truncated, infos

    def set_stage_and_level(self, stage: str, level: int) -> None:
        """Stage and curriculum level for every env's next episode (SPEC §9.9)."""
        self.sim.set_stage_and_level(stage, level)

    # The rest of SB3's VecEnv interface. The envs live in Rust with no Python-side
    # objects, so these act on this one object, for all envs at once.
    def close(self) -> None:
        """Nothing to release."""

    def get_attr(self, attr_name: str, indices: VecEnvIndices = None) -> list[Any]:
        """This object's attribute, once per requested env."""
        return [getattr(self, attr_name) for _ in self._get_indices(indices)]

    def set_attr(self, attr_name: str, value: object, indices: VecEnvIndices = None) -> None:
        """Sets an attribute for all envs (per-env attributes do not exist here)."""
        self._refuse_some_envs(indices)
        setattr(self, attr_name, value)

    def env_method(
        self,
        method_name: str,
        *method_args: object,
        indices: VecEnvIndices = None,
        **method_kwargs: object,
    ) -> list[Any]:
        """Calls one of this object's methods for all envs; its result once per env."""
        self._refuse_some_envs(indices)
        result = getattr(self, method_name)(*method_args, **method_kwargs)
        return [result for _ in range(self.num_envs)]

    def env_is_wrapped(
        self, wrapper_class: type[gym.Wrapper], indices: VecEnvIndices = None
    ) -> list[bool]:
        """No env is wrapped in a Gymnasium wrapper."""
        return [False for _ in self._get_indices(indices)]

    def _refuse_some_envs(self, indices: VecEnvIndices) -> None:
        """Settings apply to every env; asking for only some is a mistake."""
        if indices is not None and list(self._get_indices(indices)) != list(range(self.num_envs)):
            raise NotImplementedError("NineLivesVecEnv applies settings to all envs at once")


class NineLivesEnv(gym.Env):
    """One environment as a Gymnasium Env (for `check_env` and debugging only)."""

    metadata: dict[str, Any] = {"render_modes": []}
    render_mode = None

    def __init__(self, mode: str = "realistic", stage: str = "A", seed: int = 0) -> None:
        super().__init__()
        self.sim = BatchedSim(1, load_sim_config(mode, stage), seed)
        self.observation_space = observation_space()
        self.action_space = action_space()

    def reset(
        self, *, seed: int | None = None, options: dict[str, Any] | None = None
    ) -> tuple[np.ndarray, dict[str, Any]]:
        """Starts an episode; `seed` picks it, otherwise the env's own stream does."""
        super().reset(seed=seed)
        seeds = None if seed is None else np.array([seed], dtype=np.uint64)
        return self.sim.reset(seeds)[0], {}

    def step(self, action: np.ndarray) -> tuple[np.ndarray, float, bool, bool, dict[str, Any]]:
        """One control step. At the end of an episode it returns the last observation
        of that episode (the sim has already started the next one)."""
        actions = np.asarray(action, dtype=np.float32).reshape(1, ACTION_SIZE)
        observations, rewards, terminated, truncated, info = self.sim.step(actions)
        observation = observations[0]
        step_info: dict[str, Any] = {}
        if info["done"][0]:
            observation = info["terminal_obs"][0]
            step_info["nl_episode"] = finished_episode(info, 0)
        return observation, float(rewards[0]), bool(terminated[0]), bool(truncated[0]), step_info
