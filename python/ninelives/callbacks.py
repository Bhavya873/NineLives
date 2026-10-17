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
    """Feeds every finished episode to the curriculum, moves the envs up a level when
    it says so, and logs episode outcomes to TensorBoard each rollout."""

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
                self.apply_level()
        return True

    def _on_rollout_end(self) -> None:
        curriculum = self.state.curriculum
        self.logger.record("curriculum/level", curriculum.level)
        self.logger.record("curriculum/success_rate", curriculum.success_rate())
        if self.episodes:
            success, crash = success_and_crash_rates(self.episodes)
            self.logger.record("episode/success", success)
            self.logger.record("episode/crash", crash)
            self.logger.record("episode/saves", mean_of(self.episodes, "saves"))
            self.logger.record("episode/air_used_kg", mean_of(self.episodes, "air_used_kg"))
        self.episodes = []

    def apply_level(self) -> None:
        """Sends the curriculum's stage and level to every env's next episode."""
        curriculum = self.state.curriculum
        self.training_env.env_method("set_stage_and_level", curriculum.stage, curriculum.level)
        print(f"curriculum: stage {curriculum.stage} level {curriculum.level}")


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
        success = self.evaluate(normalizer)
        if success > self.state.best_eval_success:
            self.state.best_eval_success = success
            save_checkpoint(self.run_dir / "best", self.model, normalizer, self.state)
        save_checkpoint(self.run_dir / "latest", self.model, normalizer, self.state)

    def evaluate(self, normalizer: VecNormalize) -> float:
        """Success rate on the fixed eval seeds at the hardest level; also logged."""
        config = self.config
        rows = evaluate_fixed_seeds(
            self.model,
            normalizer,
            episodes=config.eval_episodes,
            first_seed=config.eval_seed,
            mode=config.mode,
            stage=config.stage,
            level=self.state.curriculum.level_count - 1,
        )
        success, crash = success_and_crash_rates(rows)
        self.logger.record("eval/success_rate", success)
        self.logger.record("eval/crash_rate", crash)
        print(f"{self.num_timesteps} steps: eval success {success:.3f}, crash {crash:.3f}")
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


def mean_of(episodes: list[dict[str, Any]], key: str) -> float:
    """The mean of one field over episode rows."""
    return float(np.mean([episode[key] for episode in episodes]))
