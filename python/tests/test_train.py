"""Training harness tests (SPEC §12, ADR-003): a 20k-step smoke train writes a
checkpoint, a resumed run carries on from its saved curriculum, and the curriculum moves
up a level only on a full window of recent successes at its own level."""

import json
from pathlib import Path

import pytest
import yaml

from ninelives.curriculum import Curriculum
from ninelives.sim_config import CONFIGS_DIR, load_sim_config
from ninelives.train import train
from ninelives_sim import BatchedSim


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


def hybrid_config(tmp_path: Path, total_steps: int) -> Path:
    """The small config trained under the Hybrid gate on Stages A and F."""
    config = yaml.safe_load(small_config(tmp_path, total_steps).read_text())
    config["controller"] = "hybrid"
    config["stages"] = ["A", "F"]
    config["curriculum"]["stage_weights"] = {"A": 0.5, "F": 0.5}
    config["curriculum"]["start_at_hardest"] = True
    path = tmp_path / "hybrid.yaml"
    path.write_text(yaml.safe_dump(config))
    return path


def test_a_hybrid_smoke_run_trains_under_the_gate_and_keeps_a_best_checkpoint(tmp_path):
    state = run(tmp_path, hybrid_config(tmp_path, 20_000))

    assert state.steps >= 20_000
    assert (tmp_path / "smoke" / "best" / "model.zip").is_file()
    # The best checkpoint was scored by the Rust Hybrid eval, so it has a real rate.
    assert 0.0 <= state.best_eval_success <= 1.0


def test_a_hybrid_run_that_climbs_a_curriculum_is_refused(tmp_path):
    path = hybrid_config(tmp_path, 1024)
    config = yaml.safe_load(path.read_text())
    config["curriculum"]["start_at_hardest"] = False
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="start_at_hardest"):
        run(tmp_path, path)


@pytest.mark.parametrize(
    ("ppo_change", "stages", "message"),
    [
        ({"activation": "relu"}, None, "tanh"),
        ({}, ["D"], "almost never tip"),
    ],
)
def test_a_hybrid_config_that_could_not_train_or_score_is_refused(
    tmp_path, ppo_change, stages, message
):
    path = hybrid_config(tmp_path, 1024)
    config = yaml.safe_load(path.read_text())
    config["ppo"].update(ppo_change)
    if stages is not None:
        config["stages"] = stages
        config["curriculum"]["stage_weights"] = {stage: 1.0 for stage in stages}
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match=message):
        run(tmp_path, path)


def test_a_hybrid_run_cannot_carry_over_a_curriculum(tmp_path):
    run(tmp_path, small_config(tmp_path, 2048))
    checkpoint = tmp_path / "smoke" / "latest"
    config = hybrid_config(tmp_path, 1024)
    args = ["--config", str(config), "--run-id", "hybrid", "--runs-dir", str(tmp_path)]

    with pytest.raises(SystemExit, match="keep-curriculum"):
        train([*args, "--init-from", str(checkpoint), "--keep-curriculum"])


def policy_weights(model_zip: Path) -> object:
    """Every policy parameter of a saved model, as one flat tensor."""
    import torch
    from stable_baselines3 import PPO

    model = PPO.load(model_zip, device="cpu")
    return torch.nn.utils.parameters_to_vector(model.policy.parameters())


def test_two_runs_from_the_same_seed_learn_the_same_weights(tmp_path):
    import torch

    config = small_config(tmp_path, 4096)
    for run_id in ("first", "second"):
        train(["--config", str(config), "--run-id", run_id, "--runs-dir", str(tmp_path)])

    first = policy_weights(tmp_path / "first" / "latest" / "model.zip")
    second = policy_weights(tmp_path / "second" / "latest" / "model.zip")
    assert torch.equal(first, second)


def test_a_policy_weight_that_turns_nan_stops_training():
    import torch
    from stable_baselines3 import PPO

    from ninelives.callbacks import check_finite_weights

    model = PPO("MlpPolicy", "Pendulum-v1", device="cpu")
    check_finite_weights(model)
    with torch.no_grad():
        model.policy.action_net.bias[0] = float("nan")

    with pytest.raises(FloatingPointError, match="action_net.bias"):
        check_finite_weights(model)


def test_the_time_limit_stops_early_and_still_saves_latest(tmp_path):
    state = run(tmp_path, small_config(tmp_path, 1_000_000), "--max-minutes", "0")

    assert state.steps < 1_000_000
    assert (tmp_path / "smoke" / "latest" / "model.zip").is_file()


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


def test_a_fresh_policy_starts_with_its_thrusters_off():
    import gymnasium as gym
    from stable_baselines3 import PPO

    from ninelives.train import start_with_action_bias

    model = PPO("MlpPolicy", gym.make("Pendulum-v1"), device="cpu")

    start_with_action_bias(model, -2.0)

    assert model.policy.action_net.bias.detach().tolist() == [-2.0]


def test_the_base_config_starts_each_throttle_at_the_edge_of_zero():
    from ninelives.train_config import load_train_config

    ppo = load_train_config(CONFIGS_DIR / "train" / "base.yaml").ppo

    assert ppo.initial_action_bias == -1.0
    assert ppo.log_std_init == 0.0


def test_noting_episodes_never_promotes_even_on_a_full_good_window():
    plan = curriculum()

    for _ in range(20):
        plan.note(episode(True))

    assert plan.levels["A"] == 0
    assert plan.success_rate("A") == 1.0


def test_a_passing_eval_moves_a_stage_up_and_a_failing_one_does_not():
    plan = curriculum(advance_at=0.85)

    assert not plan.judge_eval("A", 0.85)
    assert plan.judge_eval("A", 0.9)

    assert plan.levels["A"] == 1


def test_only_stages_whose_pass_would_change_the_mix_need_an_eval():
    plan = curriculum(levels_a=2)
    plan.pd_success = {"A": 0.5, "B": 0.5}

    assert plan.needs_eval_to_advance("A")  # below its hardest level
    plan.judge_eval("A", 1.0)
    assert plan.needs_eval_to_advance("A")  # hardest level, newest stage: unlocks B
    plan.judge_eval("A", 1.0)

    assert plan.unlocked_stages() == ["A", "B"]
    assert not plan.needs_eval_to_advance("A")
    assert plan.needs_eval_to_advance("B")


def test_an_unknown_promote_on_is_refused_before_training(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["curriculum"]["promote_on"] = "vibes"
    path = tmp_path / "bad.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="promote_on"):
        run(tmp_path, path)


def test_a_config_without_promote_on_promotes_on_eval(tmp_path):
    from ninelives.train_config import load_train_config

    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["curriculum"].pop("promote_on")
    path = tmp_path / "old.yaml"
    path.write_text(yaml.safe_dump(config))

    assert load_train_config(path).curriculum_promote_on == "eval"


def test_each_level_has_its_own_bar_and_the_last_one_repeats():
    plan = curriculum(levels_a=5)
    plan.advance_at = [0.85, 0.75]

    assert not plan.judge_eval("A", 0.80)
    assert plan.judge_eval("A", 0.86)
    assert plan.judge_eval("A", 0.80)  # level 1: bar 0.75
    assert plan.bar("A") == 0.75  # level 2 reuses the last bar


def test_the_hardest_level_unlocks_the_next_stage_once_it_matches_pd():
    plan = curriculum(levels_a=1)
    plan.pd_success = {"A": 0.58}

    assert not plan.judge_eval("A", 0.57, steps=3_000_000)
    assert plan.judge_eval("A", 0.58, steps=4_000_000)

    assert plan.unlocked_stages() == ["A", "B"]
    assert plan.last_unlock_step == 4_000_000


def test_a_stage_unlocks_on_time_when_the_newest_never_matches_pd():
    plan = curriculum()
    plan.unlock_every_steps = 15_000_000

    assert not plan.unlock_if_due(14_000_000)
    assert plan.unlock_if_due(15_000_000)
    assert not plan.unlock_if_due(20_000_000)
    assert plan.unlock_if_due(30_000_000)

    assert plan.unlocked_stages() == ["A", "B", "C"]
    assert not plan.unlock_if_due(90_000_000)


def test_reward_overrides_replace_only_the_named_weights():
    from ninelives.sim_config import override_reward_weights

    toml = "[weights]\ntilt = 0.05\nhard_landing = 1.0\n\n[shape]\nhard_landing = 7.0\n"

    out = override_reward_weights(toml, {"hard_landing": 0.0})

    assert "hard_landing = 0.0\n" in out
    assert "[shape]\nhard_landing = 7.0" in out
    with pytest.raises(ValueError, match="not a \\[weights\\] entry"):
        override_reward_weights(toml, {"no_such_weight": 1.0})


def test_reward_overrides_with_a_shape_prefix_replace_only_the_shape_entry():
    from ninelives.sim_config import override_reward_weights

    toml = "[weights]\nhard_landing = 1.0\n\n[shape]\nhard_landing = 7.0\n"

    out = override_reward_weights(toml, {"shape.hard_landing": 3.0})

    assert "[weights]\nhard_landing = 1.0" in out
    assert "[shape]\nhard_landing = 3.0" in out
    with pytest.raises(ValueError, match="not a \\[shape\\] entry"):
        override_reward_weights(toml, {"shape.no_such_entry": 1.0})


def test_validation_seeds_inside_the_test_range_are_refused(tmp_path):
    config = yaml.safe_load(small_config(tmp_path, 1024).read_text())
    config["checkpoint"]["eval_seed"] = 500
    path = tmp_path / "leaky.yaml"
    path.write_text(yaml.safe_dump(config))

    with pytest.raises(ValueError, match="test seeds"):
        run(tmp_path, path)


def test_a_warm_started_run_begins_with_the_given_checkpoints_policy(tmp_path):
    from stable_baselines3 import PPO

    run(tmp_path, small_config(tmp_path, 2048))
    source = tmp_path / "smoke" / "latest"
    config = small_config(tmp_path, 1)
    args = ["--config", str(config), "--run-id", "warm", "--runs-dir", str(tmp_path)]

    train([*args, "--init-from", str(source)])

    started = PPO.load(tmp_path / "warm" / "latest" / "model.zip", device="cpu")
    original = PPO.load(source / "model.zip", device="cpu")
    started_bias = started.policy.action_net.bias.detach()
    original_bias = original.policy.action_net.bias.detach()
    assert (started_bias - original_bias).abs().max() < 0.05


def test_keep_curriculum_carries_over_the_checkpoints_unlocked_stages(tmp_path):
    run(tmp_path, small_config(tmp_path, 2048))
    source = tmp_path / "smoke" / "latest"
    state = json.loads((source / "state.json").read_text())
    state["curriculum"]["unlocked"] = 2
    (source / "state.json").write_text(json.dumps(state))
    config = small_config(tmp_path, 1)
    args = ["--config", str(config), "--run-id", "warm", "--runs-dir", str(tmp_path)]

    result = train([*args, "--init-from", str(source), "--keep-curriculum"])

    assert result.curriculum.unlocked == 2


def test_the_shipped_flight_config_trains_under_the_gate_from_the_hardest_level():
    from ninelives.train_config import load_train_config

    config = load_train_config(CONFIGS_DIR / "train" / "flight.yaml")

    assert config.controller == "hybrid"
    assert config.curriculum_start_at_hardest
    assert "F" in config.stages


def test_the_r4_config_randomizes_half_the_episodes():
    from ninelives.train_config import load_train_config

    config = load_train_config(CONFIGS_DIR / "train" / "flight-r4.yaml")

    assert config.randomize_share == 0.5
    assert load_train_config(CONFIGS_DIR / "train" / "base.yaml").randomize_share == 1.0


def test_a_randomize_share_outside_0_to_1_is_refused(tmp_path):
    from ninelives.train_config import load_train_config

    text = (CONFIGS_DIR / "train" / "flight-r4.yaml").read_text()
    path = tmp_path / "bad.yaml"
    path.write_text(text.replace("randomize_share: 0.5", "randomize_share: 1.5"))

    with pytest.raises(ValueError, match="randomize_share"):
        load_train_config(path)


def test_the_sim_refuses_a_randomize_share_outside_0_to_1():
    config = json.loads(load_sim_config(randomize=True, randomize_share=0.5))
    config["randomize_share"] = -0.1

    with pytest.raises(ValueError, match="randomize_share"):
        BatchedSim(1, json.dumps(config), 0)
