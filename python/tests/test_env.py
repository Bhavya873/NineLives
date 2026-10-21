"""Environment tests (SPEC §12, Python): check_env passes, a batch of 4 matches 4 single
sims, auto-reset hands back the terminal observation, and bad input is refused."""

import json

import numpy as np
import pytest
from gymnasium.utils.env_checker import check_env

from ninelives.env import NineLivesEnv, NineLivesVecEnv
from ninelives.sim_config import load_sim_config
from ninelives_sim import BatchedSim, obs_schema_hash, obs_spec

STEPS = 600  # 10 s of control steps: longer than one 8 s Stage A episode.


def random_actions(rng: np.random.Generator, n_envs: int) -> np.ndarray:
    return rng.uniform(-1.0, 1.0, size=(n_envs, 10)).astype(np.float32)


def test_the_single_env_passes_gymnasiums_env_checker():
    check_env(NineLivesEnv(), skip_render_check=True)


def test_the_observation_layout_comes_from_rust_with_41_values():
    assert len(obs_spec()) == 41
    assert len(obs_schema_hash()) == 16
    assert NineLivesEnv().observation_space.shape == (41,)


def test_a_batch_of_four_matches_four_single_sims_until_each_first_episode_ends():
    config = load_sim_config()
    seeds = np.array([10, 11, 12, 13], dtype=np.uint64)
    batch = BatchedSim(4, config, 0)
    singles = [BatchedSim(1, config, 0) for _ in range(4)]
    batch_obs = batch.reset(seeds)
    for index, single in enumerate(singles):
        assert np.array_equal(single.reset(seeds[index : index + 1])[0], batch_obs[index])

    rng = np.random.default_rng(0)
    still_first_episode = [True] * 4
    for _ in range(STEPS):
        actions = random_actions(rng, 4)
        obs, rewards, terminated, truncated, info = batch.step(actions)
        for index, single in enumerate(singles):
            if not still_first_episode[index]:
                continue
            s_obs, s_rewards, s_term, s_trunc, s_info = single.step(actions[index : index + 1])
            assert rewards[index] == s_rewards[0]
            assert terminated[index] == s_term[0] and truncated[index] == s_trunc[0]
            if info["done"][index]:
                assert np.array_equal(info["terminal_obs"][index], s_info["terminal_obs"][0])
                still_first_episode[index] = False
            else:
                assert np.array_equal(obs[index], s_obs[0])
        if not any(still_first_episode):
            break
    assert not any(still_first_episode), "some episode never ended"


def test_an_ended_env_restarts_and_hands_back_its_last_observation():
    env = NineLivesVecEnv(n_envs=4, seed=1)
    env.reset()
    rng = np.random.default_rng(1)

    for _ in range(STEPS):
        obs, rewards, dones, infos = env.step(random_actions(rng, 4))
        if dones.any():
            break

    row = int(np.flatnonzero(dones)[0])
    info = infos[row]
    assert info["terminal_observation"].shape == (41,)
    # The returned observation is the new episode's first one, not the old last one.
    assert not np.array_equal(info["terminal_observation"], obs[row])
    episode = info["nl_episode"]
    assert episode["stage"] == "A"
    assert episode["control_steps"] > 0
    timed_out = episode["crash_cause"] is None and not episode["success"]
    assert info["TimeLimit.truncated"] == timed_out


def test_observations_stay_inside_the_observation_space():
    env = NineLivesVecEnv(n_envs=8, seed=2)
    obs = env.reset()
    rng = np.random.default_rng(2)
    for _ in range(120):
        assert env.observation_space.contains(obs[0])
        obs, _, _, _ = env.step(random_actions(rng, 8))


def test_bad_input_is_refused_with_a_clear_message():
    sim = BatchedSim(2, load_sim_config(), 0)
    with pytest.raises(ValueError, match="reset"):
        sim.step(np.zeros((2, 10), dtype=np.float32))
    sim.reset()
    with pytest.raises(ValueError, match="shape"):
        sim.step(np.zeros((3, 10), dtype=np.float32))
    with pytest.raises(ValueError, match="mode"):
        BatchedSim(1, load_sim_config(mode="moon"), 0)
    with pytest.raises(ValueError, match="stage"):
        sim.set_stage_and_level("Z", 0)


def test_seeding_the_vec_env_picks_its_episodes():
    first, second, other = (NineLivesVecEnv(n_envs=2) for _ in range(3))
    first.seed(5)
    second.seed(5)
    other.seed(6)

    assert np.array_equal(first.reset(), second.reset())
    assert not np.array_equal(first.reset(), other.reset())


def finished_stages(env: NineLivesVecEnv, steps: int) -> list[str]:
    """The stages of the episodes that end within `steps` control steps of zero action."""
    env.reset()
    stages = []
    actions = np.zeros((env.num_envs, 10), dtype=np.float32)
    for _ in range(steps):
        _, _, dones, infos = env.step(actions)
        stages.extend(infos[row]["nl_episode"]["stage"] for row in np.flatnonzero(dones))
    return stages


def test_a_stage_mix_runs_every_weighted_stage_and_never_a_zero_weight_one():
    env = NineLivesVecEnv(n_envs=16, seed=3)
    env.set_stage_mix([("A", 1.0, 0), ("B", 1.0, 0), ("C", 1.0, 0), ("D", 0.0, 0)])

    stages = finished_stages(env, 1200)

    assert {"A", "B", "C"} <= set(stages)
    assert "D" not in stages


def test_a_bad_stage_mix_is_refused():
    env = NineLivesVecEnv(n_envs=2)

    with pytest.raises(ValueError, match="stage"):
        env.set_stage_mix([("Z", 1.0, 0)])
    with pytest.raises(ValueError, match="weights"):
        env.set_stage_mix([("A", 0.0, 0)])


def test_the_pieces_json_comes_from_the_config_and_a_broken_one_is_refused():
    config = json.loads(load_sim_config(stage="D"))
    config["pieces_json"] = config["pieces_json"].replace('"version": 1', '"version": 2')

    with pytest.raises(ValueError, match="pieces.json"):
        BatchedSim(1, json.dumps(config), 0)


def test_a_per_mode_run_keeps_its_mode_while_everything_else_is_randomized():
    names = [row["name"] for row in obs_spec()]
    thrust, unlimited = names.index("thrust_scale"), names.index("unlimited_air")
    seeds = np.arange(32, dtype=np.uint64)
    kept = BatchedSim(32, load_sim_config("arcade", randomize=True, randomize_mode=False), 0)
    varied = BatchedSim(32, load_sim_config("arcade", randomize=True), 0)

    kept_obs = kept.reset(seeds)
    varied_obs = varied.reset(seeds)

    assert np.all(kept_obs[:, thrust] == 2.0)
    assert np.all(kept_obs[:, unlimited] == 1.0)
    assert len(np.unique(varied_obs[:, thrust])) > 1
