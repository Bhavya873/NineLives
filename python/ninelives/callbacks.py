"""SB3 callbacks for a training run: the curriculum (SPEC §9.9), periodic checkpoints
with a best-by-eval copy (SPEC §9.10), and a wall-clock limit for `--max-minutes`.
They share one `RunState`, which is what `state.json` saves."""

import time
from pathlib import Path
from typing import Any

import numpy as np
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.vec_env import VecNormalize

from ninelives.rollout import evaluate_fixed_seeds, success_and_crash_rates
from ninelives.run_files import RunState, record_progress, save_checkpoint
from ninelives.train_config import TrainConfig


class CurriculumCallback(BaseCallback):
    """Feeds every finished episode to the curriculum, sends the envs the new stage mix
    when a stage moves up a level or unlocks, and logs outcomes per stage to TensorBoard
    each rollout."""

    def __init__(self, state: RunState) -> None:
        super().__init__()
        self.state = state
        self.episodes: list[dict[str, Any]] = []

    def _on_step(self) -> bool:
        for info in self.locals["infos"]:
            episode = info.get("nl_episode")
            if episode is None:
                continue
            self.episodes.append(episode)
            if self.state.curriculum.record(episode):
                self.apply_mix()
        return True

    def _on_rollout_end(self) -> None:
        curriculum = self.state.curriculum
        self.logger.record("curriculum/unlocked", curriculum.unlocked)
        for stage in curriculum.unlocked_stages():
            self.logger.record(f"curriculum/{stage}/level", curriculum.levels[stage])
            self.logger.record(f"curriculum/{stage}/success_rate", curriculum.success_rate(stage))
        if self.episodes:
            success, crash = success_and_crash_rates(self.episodes)
            self.logger.record("episode/success", success)
            self.logger.record("episode/crash", crash)
            self.logger.record("episode/saves", mean_of(self.episodes, "saves"))
            self.logger.record("episode/air_used_kg", mean_of(self.episodes, "air_used_kg"))
            for stage, rows in rows_by_stage(self.episodes).items():
                self.logger.record(f"episode/{stage}/success", success_and_crash_rates(rows)[0])
        self.episodes = []

    def apply_mix(self) -> None:
        """Sends the curriculum's unlocked stages, weights and levels to every env."""
        mix = self.state.curriculum.mix()
        self.training_env.env_method("set_stage_mix", mix)
        print(f"curriculum: {mix}")


class RunCheckpointCallback(BaseCallback):
    """Every `checkpoint_every_steps`: saves a numbered checkpoint and `latest/`,
    evaluates the policy on fixed seeds at the hardest level and keeps `best/`."""

    def __init__(self, state: RunState, run_dir: Path, config: TrainConfig) -> None:
        super().__init__()
        self.state = state
        self.run_dir = run_dir
        self.config = config
        self.started_at = time.monotonic()
        self.wall_seconds_before = state.wall_seconds
        every = config.checkpoint_every_steps
        self.next_save = (state.steps // every + 1) * every

    def _on_step(self) -> bool:
        if self.num_timesteps >= self.next_save:
            self.save_all()
            self.next_save += self.config.checkpoint_every_steps
        return True

    def save_all(self) -> None:
        """Numbered checkpoint, eval, best (if better) and latest."""
        normalizer = self.model.get_vec_normalize_env()
        if normalizer is None:
            raise RuntimeError("training env must be wrapped in VecNormalize")
        self.record_progress(self.num_timesteps)
        numbered = self.run_dir / "checkpoints" / str(self.num_timesteps)
        save_checkpoint(numbered, self.model, normalizer)
        stages = self.state.curriculum.unlocked_stages()
        if self.state.best_eval_stages != stages:
            # A new stage joined: scores over different stage sets don't compare, so the
            # first checkpoint on the new set becomes its best.
            self.state.best_eval_stages = stages
            self.state.best_eval_success = -1.0
        success = self.evaluate(normalizer, stages)
        if success > self.state.best_eval_success:
            self.state.best_eval_success = success
            save_checkpoint(self.run_dir / "best", self.model, normalizer, self.state)
        save_checkpoint(self.run_dir / "latest", self.model, normalizer, self.state)

    def evaluate(self, normalizer: VecNormalize, stages: list[str]) -> float:
        """Success rate over the episodes of `stages` (the unlocked ones), each on the
        fixed eval seeds at its hardest level; also logged per stage."""
        config = self.config
        curriculum = self.state.curriculum
        all_rows = []
        for stage in stages:
            rows = evaluate_fixed_seeds(
                self.model,
                normalizer,
                episodes=config.eval_episodes,
                first_seed=config.eval_seed,
                mode=config.mode,
                stage=stage,
                level=curriculum.level_counts[stage] - 1,
            )
            self.logger.record(f"eval/{stage}/success_rate", success_and_crash_rates(rows)[0])
            all_rows.extend(rows)
        success, crash = success_and_crash_rates(all_rows)
        self.logger.record("eval/success_rate", success)
        self.logger.record("eval/crash_rate", crash)
        covered = ", ".join(stages)
        print(
            f"{self.num_timesteps} steps: eval success {success:.3f}, crash {crash:.3f} "
            f"(stages {covered})"
        )
        return success

    def record_progress(self, steps: int) -> None:
        """Records `steps` and this run's total wall time in the shared state."""
        record_progress(self.state, steps, self.wall_seconds_before, self.started_at)


class TimeLimitCallback(BaseCallback):
    """Stops training once `max_minutes` of wall time have passed (None: never)."""

    def __init__(self, max_minutes: float | None) -> None:
        super().__init__()
        self.deadline = None if max_minutes is None else time.monotonic() + 60.0 * max_minutes

    def _on_step(self) -> bool:
        return self.deadline is None or time.monotonic() < self.deadline


def rows_by_stage(episodes: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    """Episode rows grouped by their stage."""
    groups: dict[str, list[dict[str, Any]]] = {}
    for episode in episodes:
        groups.setdefault(episode["stage"], []).append(episode)
    return groups


def mean_of(episodes: list[dict[str, Any]], key: str) -> float:
    """The mean of one field over episode rows."""
    return float(np.mean([episode[key] for episode in episodes]))
