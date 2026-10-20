"""SB3 callbacks for a training run: the curriculum (SPEC §9.9), periodic checkpoints
with a best-by-eval copy (SPEC §9.10), and a wall-clock limit for `--max-minutes`.
They share one `RunState`, which is what `state.json` saves."""

import time
from pathlib import Path
from typing import Any

import numpy as np
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.vec_env import VecEnv, VecNormalize

from ninelives.rollout import evaluate_fixed_seeds, success_and_crash_rates
from ninelives.run_files import RunState, record_progress, save_checkpoint
from ninelives.train_config import TrainConfig


class CurriculumCallback(BaseCallback):
    """Feeds every finished episode to the curriculum, logs outcomes per stage to
    TensorBoard each rollout and, with `promote_from_training`, promotes stages from the
    training window and sends the envs the new mix. Otherwise `RunCheckpointCallback`
    promotes them from its deterministic eval."""

    def __init__(self, state: RunState, promote_from_training: bool) -> None:
        super().__init__()
        self.state = state
        self.promote_from_training = promote_from_training
        self.episodes: list[dict[str, Any]] = []

    def _on_step(self) -> bool:
        for info in self.locals["infos"]:
            episode = info.get("nl_episode")
            if episode is None:
                continue
            self.episodes.append(episode)
            if not self.promote_from_training:
                self.state.curriculum.note(episode)
            elif self.state.curriculum.record(episode):
                send_mix(self.training_env, self.state)
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


def send_mix(training_env: VecEnv, state: RunState) -> None:
    """Sends the curriculum's unlocked stages, weights and levels to every env."""
    mix = state.curriculum.mix()
    training_env.env_method("set_stage_mix", mix)
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
        if self.config.curriculum_promote_on == "eval":
            self.promote_from_eval(normalizer)
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

    def promote_from_eval(self, normalizer: VecNormalize) -> None:
        """Promotes each stage whose deterministic eval at its current level passes the
        curriculum bar, and sends the envs the new mix if anything moved."""
        curriculum = self.state.curriculum
        changed = False
        for stage in curriculum.unlocked_stages():
            if not curriculum.needs_eval_to_advance(stage):
                continue
            level = curriculum.levels[stage]
            rows = self.eval_rows(normalizer, stage, level, self.promotion_seed())
            success = success_and_crash_rates(rows)[0]
            self.logger.record(f"curriculum/{stage}/eval_success", success)
            print(f"{self.num_timesteps} steps: {stage} level {level} eval success {success:.3f}")
            changed = curriculum.judge_eval(stage, success, self.num_timesteps) or changed
        changed = curriculum.unlock_if_due(self.num_timesteps) or changed
        if changed:
            send_mix(self.training_env, self.state)

    def promotion_seed(self) -> int:
        """First seed of this checkpoint's promotion eval: a fresh block per checkpoint,
        past the best-checkpoint seeds, so a stage is never promoted for fitting one
        fixed sample of drops."""
        checkpoint = self.num_timesteps // self.config.checkpoint_every_steps
        return self.config.eval_seed + (checkpoint + 1) * self.config.eval_episodes

    def eval_rows(
        self, normalizer: VecNormalize, stage: str, level: int, first_seed: int | None = None
    ) -> list[dict[str, Any]]:
        """`stage` at `level` on fixed seeds from `first_seed` (default: the
        best-checkpoint eval seeds), acting deterministically."""
        return evaluate_fixed_seeds(
            self.model,
            normalizer,
            episodes=self.config.eval_episodes,
            first_seed=self.config.eval_seed if first_seed is None else first_seed,
            mode=self.config.mode,
            stage=stage,
            level=level,
            residual=residual_json(self.config),
        )

    def evaluate(self, normalizer: VecNormalize, stages: list[str]) -> float:
        """Success rate over the episodes of `stages` (the unlocked ones), each on the
        fixed eval seeds at its hardest level; also logged per stage."""
        curriculum = self.state.curriculum
        all_rows = []
        for stage in stages:
            rows = self.eval_rows(normalizer, stage, curriculum.level_counts[stage] - 1)
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


def residual_json(config: TrainConfig) -> dict[str, object] | None:
    """The run's residual setting as the sim config wants it (None without one)."""
    return None if config.residual is None else config.residual.as_sim_json()


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
