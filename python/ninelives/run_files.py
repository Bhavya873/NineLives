"""Where a training run keeps its files, and how checkpoints are saved and loaded.

    runs/<run-id>/
      latest/            model.zip, vecnormalize.pkl, state.json (resume from here)
      best/              the same, for the best eval success so far
      checkpoints/<n>/   model.zip, vecnormalize.pkl every `every_steps` steps
      configs/           the configs/*.toml the run started with
      tb/                TensorBoard logs

`runs/` is gitignored and stays on the training machine (ADR-003). state.json holds
the step count, curriculum progress, best eval success and wall time.
"""

import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import VecEnv, VecNormalize

from ninelives.curriculum import Curriculum
from ninelives.env import NineLivesVecEnv
from ninelives.sim_config import REPO_ROOT

RUNS_DIR = REPO_ROOT / "runs"
# The exported policy the game and `eval --controller ai` load (SPEC §9.12).
DEFAULT_POLICY_PATH = REPO_ROOT / "models" / "policy.json"
MODEL_FILE = "model.zip"
NORMALIZER_FILE = "vecnormalize.pkl"
STATE_FILE = "state.json"


@dataclass
class RunState:
    """What a run knows besides the network and normalizer. `steps` repeats the model's
    own count so state.json can be read without loading the model."""

    steps: int
    # The run's planned length; the learning rate decays over it.
    total_steps: int
    curriculum: Curriculum
    best_eval_success: float
    wall_seconds: float


def record_progress(
    state: RunState, steps: int, wall_seconds_before: float, started_at: float
) -> None:
    """Sets the step count and the run's total wall time (earlier chunks plus this one)."""
    state.steps = steps
    state.wall_seconds = wall_seconds_before + time.monotonic() - started_at


def save_checkpoint(
    folder: Path, model: PPO, normalizer: VecNormalize, state: RunState | None = None
) -> None:
    """Writes the model, the normalizer statistics and (if given) the run state."""
    folder.mkdir(parents=True, exist_ok=True)
    model.save(folder / MODEL_FILE)
    normalizer.save(str(folder / NORMALIZER_FILE))
    if state is not None:
        (folder / STATE_FILE).write_text(json.dumps(state_to_dict(state), indent=2))


def load_checkpoint(
    folder: Path, env: VecEnv, curriculum: Curriculum
) -> tuple[PPO, VecNormalize, RunState]:
    """The model, normalizer (wrapping `env`, still training) and state in `folder`.
    `curriculum` is built from the current config; the saved progress is applied to it."""
    if not (folder / STATE_FILE).is_file():
        raise FileNotFoundError(f"no checkpoint to resume in {folder}")
    normalizer = VecNormalize.load(str(folder / NORMALIZER_FILE), env)
    normalizer.training = True
    model = PPO.load(folder / MODEL_FILE, env=normalizer, device="cpu")
    saved = json.loads((folder / STATE_FILE).read_text())
    curriculum.restore(saved["curriculum"])
    state = RunState(
        steps=saved["steps"],
        total_steps=saved.get("total_steps", 0),
        curriculum=curriculum,
        best_eval_success=saved["best_eval_success"],
        wall_seconds=saved["wall_seconds"],
    )
    return model, normalizer, state


def load_model_and_normalizer(folder: Path) -> tuple[PPO, VecNormalize]:
    """The model and normalizer statistics saved in a checkpoint folder (for export).
    SB3 needs an env to attach the statistics to; a one-env sim stands in."""
    if not (folder / MODEL_FILE).is_file():
        raise FileNotFoundError(f"no {MODEL_FILE} in {folder}")
    model = PPO.load(folder / MODEL_FILE, device="cpu")
    normalizer = VecNormalize.load(str(folder / NORMALIZER_FILE), NineLivesVecEnv(n_envs=1))
    normalizer.training = False
    return model, normalizer


def state_to_dict(state: RunState) -> dict[str, Any]:
    """state.json's contents."""
    return {
        "steps": state.steps,
        "total_steps": state.total_steps,
        "curriculum": state.curriculum.progress(),
        "best_eval_success": state.best_eval_success,
        "wall_seconds": state.wall_seconds,
    }
