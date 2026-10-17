"""Training harness tests (SPEC §12, ADR-003): a 20k-step smoke train writes a
checkpoint, a resumed run carries on from its saved curriculum, and the curriculum moves
up a level only on a full window of recent successes at its own level."""

import json
from pathlib import Path

import pytest
import yaml

from ninelives.curriculum import Curriculum
from ninelives.sim_config import CONFIGS_DIR
from ninelives.train import train


def small_config(tmp_path: Path, total_steps: int) -> Path:
    """The base config shrunk so a run takes seconds."""
    config = yaml.safe_load((CONFIGS_DIR / "train" / "base.yaml").read_text())
    config["total_steps"] = total_steps
    config["ppo"].update({"n_envs": 8, "n_steps": 128, "batch_size": 256, "n_epochs": 2})
    config["checkpoint"].update({"every_steps": 10_000, "eval_episodes": 4})
    path = tmp_path / "small.yaml"
    path.write_text(yaml.safe_dump(config))
    return path


def run(tmp_path: Path, config: Path, *extra: str) -> object:
    """Trains (or resumes, with `--resume` in `extra`) the run "smoke" under tmp_path."""
    args = ["--config", str(config), "--run-id", "smoke", "--runs-dir", str(tmp_path)]
    return train([*args, *extra])


def test_a_20k_step_smoke_run_writes_a_checkpoint(tmp_path):
    state = run(tmp_path, small_config(tmp_path, 20_000))

    run_dir = tmp_path / "smoke"
    assert state.steps >= 20_000
    for name in ("model.zip", "vecnormalize.pkl", "state.json"):
        assert (run_dir / "latest" / name).is_file()
        assert (run_dir / "best" / name).is_file()
    assert len(list((run_dir / "checkpoints").glob("*/model.zip"))) == 2


def test_a_resumed_run_carries_on_from_its_saved_step_and_level(tmp_path):
    run(tmp_path, small_config(tmp_path, 4096))
    state_file = tmp_path / "smoke" / "latest" / "state.json"
    saved = json.loads(state_file.read_text())
    saved["curriculum"]["level"] = 2
    state_file.write_text(json.dumps(saved))

    state = run(tmp_path, small_config(tmp_path, 8192), "--resume")

    assert state.steps >= 8192
    assert state.curriculum.level >= 2


def test_starting_over_an_existing_run_is_refused(tmp_path):
    config = small_config(tmp_path, 1024)
    run(tmp_path, config)

    with pytest.raises(SystemExit, match="--resume"):
        run(tmp_path, config)


def episode(success: bool, level: int = 0) -> dict:
    return {"stage": "A", "level": level, "success": success}


def test_the_curriculum_moves_up_only_on_a_full_good_window():
    curriculum = Curriculum(stage="A", level=0, level_count=5, window=10, advance_at=0.85)

    for _ in range(9):
        assert not curriculum.record(episode(True))
    assert curriculum.record(episode(True))

    assert curriculum.level == 1
    assert curriculum.success_rate() == 0.0


def test_the_curriculum_ignores_episodes_from_an_older_level_and_stops_at_the_top():
    curriculum = Curriculum(stage="A", level=4, level_count=5, window=2, advance_at=0.5)

    assert not curriculum.record(episode(True, level=3))
    assert not curriculum.record(episode(True, level=4))
    assert not curriculum.record(episode(True, level=4))

    assert curriculum.level == 4
    assert len(curriculum.recent_successes) == 2


def test_saved_curriculum_progress_restores_onto_the_configured_settings():
    curriculum = Curriculum(stage="A", level=1, level_count=5, window=3, advance_at=0.85)
    curriculum.record(episode(True, level=1))
    curriculum.record(episode(False, level=1))
    restored = Curriculum(stage="A", level=0, level_count=5, window=3, advance_at=0.85)

    restored.restore(json.loads(json.dumps(curriculum.progress())))

    assert restored == curriculum


def test_a_restored_level_above_the_last_is_lowered_and_its_window_dropped():
    curriculum = Curriculum(stage="A", level=0, level_count=3, window=5, advance_at=0.85)

    curriculum.restore({"stage": "A", "level": 7, "recent_successes": [True, True]})

    assert curriculum.level == 2
    assert curriculum.recent_successes == []


def test_resuming_a_checkpoint_of_another_stage_is_refused():
    curriculum = Curriculum(stage="A", level=0, level_count=3, window=5, advance_at=0.85)

    with pytest.raises(ValueError, match="stage B"):
        curriculum.restore({"stage": "B", "level": 0, "recent_successes": []})


def test_a_bad_activation_is_refused_before_training(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["ppo"]["activation"] = "gelu"
    path = tmp_path / "bad.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="activation"):
        run(tmp_path, path)
