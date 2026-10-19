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
    saved["curriculum"]["levels"]["A"] = 2
    state_file.write_text(json.dumps(saved))

    state = run(tmp_path, small_config(tmp_path, 8192), "--resume")

    assert state.steps >= 8192
    assert state.curriculum.levels["A"] >= 2


def test_starting_over_an_existing_run_is_refused(tmp_path):
    config = small_config(tmp_path, 1024)
    run(tmp_path, config)

    with pytest.raises(SystemExit, match="--resume"):
        run(tmp_path, config)


def episode(success: bool, level: int = 0, stage: str = "A") -> dict:
    return {"stage": stage, "level": level, "success": success}


def curriculum(window: int = 10, levels_a: int = 5, advance_at: float = 0.85) -> Curriculum:
    """Stages A (with levels), B and C; window and bar as given."""
    return Curriculum(
        stages=["A", "B", "C"],
        weights={"A": 0.5, "B": 0.3, "C": 0.2},
        level_counts={"A": levels_a, "B": 1, "C": 1},
        window=window,
        advance_at=advance_at,
    )


def test_the_curriculum_moves_up_only_on_a_full_good_window():
    plan = curriculum()

    for _ in range(9):
        assert not plan.record(episode(True))
    assert plan.record(episode(True))

    assert plan.levels["A"] == 1
    assert plan.success_rate("A") == 0.0


def test_the_curriculum_ignores_episodes_from_an_older_level_or_a_locked_stage():
    plan = curriculum(window=2)
    plan.levels["A"] = 2

    assert not plan.record(episode(True, level=1))
    assert not plan.record(episode(True, stage="B"))

    assert plan.recent["A"] == []
    assert plan.recent["B"] == []


def test_passing_the_last_level_unlocks_the_next_stage_and_adds_it_to_the_mix():
    plan = curriculum(window=2, levels_a=1)
    assert plan.mix() == [("A", 0.5, 0)]

    plan.record(episode(True))
    unlocked = plan.record(episode(True))

    assert unlocked
    assert plan.unlocked_stages() == ["A", "B"]
    assert plan.mix() == [("A", 0.5, 0), ("B", 0.3, 0)]


def test_an_older_stage_passing_again_does_not_unlock_more():
    plan = curriculum(window=1, levels_a=1)
    plan.record(episode(True))  # unlocks B

    assert not plan.record(episode(True))

    assert plan.unlocked_stages() == ["A", "B"]


def test_the_last_stage_passing_unlocks_nothing_more():
    plan = curriculum(window=1, levels_a=1)
    plan.unlocked = 3

    assert not plan.record(episode(True, stage="C"))
    assert plan.unlocked == 3


def test_saved_curriculum_progress_restores_onto_the_configured_settings():
    plan = curriculum(window=3)
    plan.unlocked = 2
    plan.levels["A"] = 1
    plan.record(episode(True, level=1))
    plan.record(episode(False, stage="B"))
    restored = curriculum(window=3)

    restored.restore(json.loads(json.dumps(plan.progress())))

    assert restored == plan


def test_a_restored_level_above_the_last_is_lowered_and_its_window_dropped():
    plan = curriculum(window=5, levels_a=3)
    progress = plan.progress()
    progress["levels"] = {"A": 7, "B": 0, "C": 0}
    progress["recent"] = {"A": [True, True], "B": [True], "C": []}

    plan.restore(progress)

    assert plan.levels["A"] == 2
    assert plan.recent["A"] == []
    assert plan.recent["B"] == [True]


def test_resuming_a_checkpoint_of_other_stages_is_refused():
    plan = curriculum()
    progress = plan.progress()
    progress["stages"] = ["A"]

    with pytest.raises(ValueError, match="stages"):
        plan.restore(progress)


def test_an_m5_single_stage_checkpoint_resumes_as_its_stage_alone():
    plan = curriculum(window=5)

    plan.restore({"stage": "A", "level": 3, "recent_successes": [True, False]})

    assert plan.unlocked_stages() == ["A"]
    assert plan.levels == {"A": 3, "B": 0, "C": 0}
    assert plan.recent["A"] == [True, False]


def test_an_unreadable_checkpoint_curriculum_gives_a_readable_error():
    plan = curriculum()

    with pytest.raises(ValueError, match="start a new run"):
        plan.restore({"level": 1})
    with pytest.raises(ValueError, match="stage B alone"):
        plan.restore({"stage": "B", "level": 0, "recent_successes": []})


def test_a_zero_weight_stage_is_refused_before_training(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["curriculum"]["stage_weights"]["C"] = 0.0
    path = tmp_path / "zero_weight.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="weight above 0 for stage C"):
        run(tmp_path, path)


def test_an_unknown_stage_is_refused_before_training(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["stages"] = ["A", "Z"]
    path = tmp_path / "bad_stage.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="'Z'"):
        run(tmp_path, path)


def test_a_bad_activation_is_refused_before_training(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["ppo"]["activation"] = "gelu"
    path = tmp_path / "bad.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="activation"):
        run(tmp_path, path)


def test_a_quoted_randomize_flag_is_refused_instead_of_meaning_true(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["randomize"] = "false"
    path = tmp_path / "quoted.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="randomize must be true or false"):
        run(tmp_path, path)
