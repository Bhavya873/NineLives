"""The training curriculum (SPEC §9.9): watch each finished episode's success at the
current level, and move up a level once the recent success rate is high enough.

Only Stage A has levels in M5. The level count comes from the sim (scenarios.toml), the
window and bar from the training config; only the progress (`stage`, `level`, recent
successes) is saved with each checkpoint, so a resumed run carries on where it was.
"""

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Curriculum:
    """Where training is, and the recent successes at that level."""

    stage: str
    level: int
    level_count: int
    window: int
    advance_at: float
    recent_successes: list[bool] = field(default_factory=list)

    def record(self, episode: dict[str, Any]) -> bool:
        """Counts one finished episode (BatchedSim's episode row). Returns True when this
        moved the curriculum up a level. Episodes from an older level are ignored."""
        if episode["stage"] != self.stage or episode["level"] != self.level:
            return False
        self.recent_successes.append(bool(episode["success"]))
        self.recent_successes = self.recent_successes[-self.window :]
        if self.is_last_level() or not self.window_is_good():
            return False
        self.level += 1
        self.recent_successes = []
        return True

    def success_rate(self) -> float:
        """Success rate over the recent episodes at this level (0 with none yet)."""
        if not self.recent_successes:
            return 0.0
        return sum(self.recent_successes) / len(self.recent_successes)

    def window_is_good(self) -> bool:
        """A full window whose success rate passes the bar."""
        full = len(self.recent_successes) >= self.window
        return full and self.success_rate() > self.advance_at

    def is_last_level(self) -> bool:
        """True at the stage's hardest level."""
        return self.level >= self.level_count - 1

    def progress(self) -> dict[str, Any]:
        """What a checkpoint saves: the stage, level and recent successes."""
        return {
            "stage": self.stage,
            "level": self.level,
            "recent_successes": self.recent_successes,
        }

    def restore(self, progress: dict[str, Any]) -> None:
        """Carries on from saved `progress` (settings stay as configured now). A level
        above the configured last level is lowered to it, which also starts its window
        afresh."""
        if progress["stage"] != self.stage:
            raise ValueError(
                f"checkpoint was training stage {progress['stage']}, but the config says "
                f"stage {self.stage}; resume with the same stage or start a new run"
            )
        last_level = self.level_count - 1
        self.level = min(progress["level"], last_level)
        self.recent_successes = []
        if progress["level"] == self.level:
            self.recent_successes = list(progress["recent_successes"])[-self.window :]
