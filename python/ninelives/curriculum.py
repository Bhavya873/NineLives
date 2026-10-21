"""The training curriculum (SPEC §9.9): watch each finished episode's success per stage,
move a stage up a level once its recent success rate is high enough, and unlock the next
stage once the newest one passes at its last level. Unlocked stages are then sampled by
weight (in Rust, from each episode's seed), so earlier skills are not forgotten.

Level counts come from the sim (scenarios.toml); the stage order, weights, window and bar
from the training config. Only the progress (unlocked count, levels, recent successes)
is saved with each checkpoint, so a resumed run carries on where it was.
"""

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Curriculum:
    """Which stages are unlocked, each stage's level, and its recent successes there."""

    stages: list[str]
    weights: dict[str, float]
    level_counts: dict[str, int]
    window: int
    advance_at: float
    unlocked: int = 1
    levels: dict[str, int] = field(default_factory=dict)
    recent: dict[str, list[bool]] = field(default_factory=dict)

    def __post_init__(self) -> None:
        for stage in self.stages:
            self.levels.setdefault(stage, 0)
            self.recent.setdefault(stage, [])

    def record(self, episode: dict[str, Any]) -> bool:
        """Counts one finished episode (BatchedSim's episode row) and promotes its stage
        when the window passes. Returns True when this changed the mix: a stage moved up
        a level or the next stage unlocked."""
        if not self.note(episode):
            return False
        stage = episode["stage"]
        return self.window_is_good(stage) and self.advance(stage)

    def note(self, episode: dict[str, Any]) -> bool:
        """Adds one finished episode to its stage's window, without promoting. Episodes
        from a locked stage or an older level are ignored (returns False)."""
        stage = episode["stage"]
        if stage not in self.unlocked_stages() or episode["level"] != self.levels[stage]:
            return False
        window = self.recent[stage]
        window.append(bool(episode["success"]))
        del window[: -self.window]
        return True

    def judge_eval(self, stage: str, success_rate: float) -> bool:
        """Promotes `stage` if its deterministic eval at its current level passes the bar
        (`promote_on: eval`). Returns True when this changed the mix."""
        return success_rate > self.advance_at and self.advance(stage)

    def needs_eval_to_advance(self, stage: str) -> bool:
        """True while passing `stage` would change the mix: below its hardest level, or
        the newest unlocked stage with more stages still locked."""
        if not self.is_last_level(stage):
            return True
        newest = stage == self.unlocked_stages()[-1]
        return newest and self.unlocked < len(self.stages)

    def advance(self, stage: str) -> bool:
        """Moves `stage` up a level, or unlocks the next stage if `stage` is the newest
        one at its hardest level. Returns True when this changed the mix."""
        if not self.is_last_level(stage):
            self.levels[stage] += 1
            self.recent[stage] = []
            return True
        if stage == self.unlocked_stages()[-1] and self.unlocked < len(self.stages):
            self.unlocked += 1
            return True
        return False

    def unlocked_stages(self) -> list[str]:
        """The stages training samples from now, in unlock order."""
        return self.stages[: self.unlocked]

    def mix(self) -> list[tuple[str, float, int]]:
        """`(stage, weight, level)` for every unlocked stage: what BatchedSim samples."""
        return [
            (stage, self.weights[stage], self.levels[stage]) for stage in self.unlocked_stages()
        ]

    def success_rate(self, stage: str) -> float:
        """`stage`'s success rate over its recent episodes at its level (0 with none)."""
        window = self.recent[stage]
        return sum(window) / len(window) if window else 0.0

    def window_is_good(self, stage: str) -> bool:
        """A full window whose success rate passes the bar."""
        full = len(self.recent[stage]) >= self.window
        return full and self.success_rate(stage) > self.advance_at

    def is_last_level(self, stage: str) -> bool:
        """True at the stage's hardest level."""
        return self.levels[stage] >= self.level_counts[stage] - 1

    def progress(self) -> dict[str, Any]:
        """What a checkpoint saves: the stage order, unlocked count, levels and windows."""
        return {
            "stages": self.stages,
            "unlocked": self.unlocked,
            "levels": self.levels,
            "recent": self.recent,
        }

    def restore(self, progress: dict[str, Any]) -> None:
        """Carries on from saved `progress` (settings stay as configured now). A level
        above a stage's configured last level is lowered to it, which also starts that
        stage's window afresh. Progress saved by the single-stage M5 curriculum
        (`stage`, `level`, `recent_successes`) is read as that stage alone unlocked."""
        if "stages" not in progress:
            progress = upgrade_single_stage_progress(progress, self.stages)
        if progress["stages"] != self.stages:
            raise ValueError(
                f"checkpoint was training stages {progress['stages']}, but the config says "
                f"{self.stages}; resume with the same stages or start a new run"
            )
        self.unlocked = min(int(progress["unlocked"]), len(self.stages))
        for stage in self.stages:
            saved_level = int(progress["levels"][stage])
            self.levels[stage] = min(saved_level, self.level_counts[stage] - 1)
            self.recent[stage] = []
            if saved_level == self.levels[stage]:
                self.recent[stage] = list(progress["recent"][stage])[-self.window :]


def upgrade_single_stage_progress(progress: dict[str, Any], stages: list[str]) -> dict[str, Any]:
    """M5 checkpoints saved `{stage, level, recent_successes}` for one stage. That reads
    as the configured stages with only the first unlocked, if the old stage is it."""
    if "stage" not in progress:
        raise ValueError(
            "checkpoint's curriculum progress has neither `stages` (M7) nor `stage` (M5); "
            "the state.json is damaged or from another program: start a new run"
        )
    if progress["stage"] != stages[0]:
        raise ValueError(
            f"checkpoint was training stage {progress['stage']} alone (M5 format), but the "
            f"config starts with stages {stages}; resume with stages starting at "
            f"{progress['stage']} or start a new run"
        )
    levels = {stage: 0 for stage in stages}
    recent: dict[str, list[bool]] = {stage: [] for stage in stages}
    levels[stages[0]] = int(progress["level"])
    recent[stages[0]] = list(progress["recent_successes"])
    return {"stages": stages, "unlocked": 1, "levels": levels, "recent": recent}
