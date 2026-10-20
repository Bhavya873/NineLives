"""Residual control tests (M7 approach 2): a zero residual action flies exactly like PD,
the `residual` training setting is read and checked, and a residual run starts with a
zeroed action head and every stage at its hardest level."""

import json
from pathlib import Path

import numpy as np
import pytest
import yaml

from ninelives.sim_config import CONFIGS_DIR, load_sim_config
from ninelives.train import train
from ninelives.train_config import ResidualSettings, load_train_config
from ninelives_sim import BatchedSim, run_eval

PD_RESIDUAL = {"base": "pd", "scale": 0.5}


def run_zero_residual(stage: str, first_seed: int, episodes: int) -> list[dict]:
    """Episode rows of a residual BatchedSim whose policy always answers 0."""
    sim = BatchedSim(episodes, load_sim_config("realistic", stage, residual=PD_RESIDUAL), 0)
    sim.reset(np.arange(first_seed, first_seed + episodes, dtype=np.uint64))
    rows: list[dict | None] = [None] * episodes
    zeros = np.zeros((episodes, 10), dtype=np.float32)
    while None in rows:
        _, _, _, _, info = sim.step(zeros)
        for row in np.flatnonzero(info["done"]):
            if rows[row] is None:
                rows[row] = json.loads(info["episode"][row])
    return rows


@pytest.mark.parametrize("stage", ["B", "D"])
def test_a_zero_residual_action_flies_exactly_like_pd(stage):
    episodes, first_seed = 6, 2_000_000
    config = load_sim_config("realistic", stage)
    report = json.loads(run_eval("pd", config, episodes, first_seed))

    rows = run_zero_residual(stage, first_seed, episodes)

    for pd_row, residual_row in zip(report["episodes"], rows, strict=True):
        assert residual_row["success"] == pd_row["success"]
        assert residual_row["crash_cause"] == pd_row["crash_cause"]
        assert residual_row["air_used_kg"] == pytest.approx(pd_row["air_used_kg"])


def test_a_residual_base_other_than_pd_is_refused_by_the_sim():
    config = load_sim_config("realistic", "A", residual={"base": "off", "scale": 0.5})

    with pytest.raises(ValueError, match="residual.base"):
        BatchedSim(1, config, 0)


def write_config(tmp_path: Path, **changes: object) -> Path:
    """The base config with top-level `changes` and a few settings shrunk for speed."""
    config = yaml.safe_load((CONFIGS_DIR / "train" / "base.yaml").read_text())
    config.update(changes)
    config["ppo"].update({"n_envs": 8, "n_steps": 128, "batch_size": 256, "n_epochs": 2})
    config["checkpoint"].update({"every_steps": 10_000, "eval_episodes": 4})
    path = tmp_path / "residual.yaml"
    path.write_text(yaml.safe_dump(config))
    return path


def residual_config(tmp_path: Path) -> Path:
    """A residual run: PD plus half a throttle per unit action, zero bias, all stages
    unlocked at their hardest levels."""
    config = yaml.safe_load(write_config(tmp_path, residual=PD_RESIDUAL).read_text())
    config["ppo"]["initial_action_bias"] = 0.0
    config["ppo"]["log_std_init"] = -1.0
    config["curriculum"]["start_at_hardest"] = True
    path = tmp_path / "residual.yaml"
    path.write_text(yaml.safe_dump(config))
    return path


def test_the_residual_setting_is_read_from_the_training_yaml(tmp_path):
    config = load_train_config(residual_config(tmp_path))

    assert config.residual == ResidualSettings(base="pd", scale=0.5)
    assert config.curriculum_start_at_hardest is True


def test_a_config_without_residual_trains_the_action_as_the_throttle(tmp_path):
    config = load_train_config(write_config(tmp_path))

    assert config.residual is None
    assert config.curriculum_start_at_hardest is False


@pytest.mark.parametrize(
    ("residual", "message"),
    [
        ({"base": "off", "scale": 0.5}, "residual.base"),
        ({"base": "pd", "scale": 0.0}, "residual.scale"),
        ({"base": "pd", "scale": 0.5, "gain": 2}, "unknown residual settings"),
    ],
)
def test_a_bad_residual_setting_is_refused_before_training(tmp_path, residual, message):
    path = write_config(tmp_path, residual=residual)
    raw = yaml.safe_load(path.read_text())
    raw["ppo"]["initial_action_bias"] = 0.0
    path.write_text(yaml.safe_dump(raw))

    with pytest.raises(ValueError, match=message):
        load_train_config(path)


def test_a_residual_run_with_an_action_bias_is_refused(tmp_path):
    path = write_config(tmp_path, residual=PD_RESIDUAL)  # base.yaml's bias is −1

    with pytest.raises(ValueError, match="initial_action_bias"):
        load_train_config(path)


def test_a_residual_run_starts_as_pd_with_every_stage_at_its_hardest_level(tmp_path):
    args = ["--config", str(residual_config(tmp_path)), "--run-id", "res"]
    state = train([*args, "--runs-dir", str(tmp_path), "--total-steps", "1024"])

    curriculum = state.curriculum
    assert curriculum.unlocked_stages() == ["A", "B", "C", "D", "E"]
    for stage in curriculum.stages:
        assert curriculum.levels[stage] == curriculum.level_counts[stage] - 1
