"""Training run settings, read from `configs/train/*.yaml` (SPEC §9.10)."""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

# Activation names `ppo.activation` may use (train.py maps them to torch layers).
ACTIVATION_NAMES = ("tanh", "relu")
# The scenario stages the sim knows (SPEC §9.6).
STAGE_NAMES = ("A", "B", "C", "D", "E")


@dataclass
class PpoSettings:
    """SB3 PPO hyperparameters."""

    n_envs: int
    n_steps: int
    batch_size: int
    n_epochs: int
    learning_rate: float
    gamma: float
    gae_lambda: float
    clip_range: float
    ent_coef: float
    net_arch: list[int]
    activation: str


@dataclass
class TrainConfig:
    """One training run's settings."""

    seed: int
    mode: str
    # Unlock order (SPEC §9.9); training starts with the first.
    stages: list[str]
    # How often each unlocked stage comes up.
    stage_weights: dict[str, float]
    randomize: bool
    total_steps: int
    ppo: PpoSettings
    clip_obs: float
    curriculum_window: int
    curriculum_advance_at: float
    checkpoint_every_steps: int
    eval_episodes: int
    eval_seed: int


def load_train_config(path: Path) -> TrainConfig:
    """Reads a training YAML; a missing key (or an unknown key under `ppo`) raises an
    error naming it."""
    raw: dict[str, Any] = yaml.safe_load(path.read_text())
    try:
        config = TrainConfig(
            seed=int(raw["seed"]),
            mode=str(raw["mode"]),
            stages=[str(stage) for stage in raw["stages"]],
            stage_weights={
                str(stage): float(weight)
                for stage, weight in raw["curriculum"]["stage_weights"].items()
            },
            randomize=strict_bool("randomize", raw["randomize"]),
            total_steps=int(raw["total_steps"]),
            ppo=read_ppo(raw["ppo"]),
            clip_obs=float(raw["normalize"]["clip_obs"]),
            curriculum_window=int(raw["curriculum"]["window"]),
            curriculum_advance_at=float(raw["curriculum"]["advance_at"]),
            checkpoint_every_steps=int(raw["checkpoint"]["every_steps"]),
            eval_episodes=int(raw["checkpoint"]["eval_episodes"]),
            eval_seed=int(raw["checkpoint"]["eval_seed"]),
        )
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{path}: missing or invalid setting: {error}") from error
    check_ranges(path, config)
    return config


def strict_bool(name: str, value: object) -> bool:
    """`value` if it is true or false; a quoted "false" must not quietly mean True."""
    if not isinstance(value, bool):
        raise ValueError(f"{name} must be true or false, not {value!r}")
    return value


def read_ppo(raw: dict[str, Any]) -> PpoSettings:
    """The `ppo` section with every number converted (YAML reads `3e-4` as text)."""
    unknown = set(raw) - set(PpoSettings.__dataclass_fields__)
    if unknown:
        raise ValueError(f"unknown ppo settings {sorted(unknown)}")
    return PpoSettings(
        n_envs=int(raw["n_envs"]),
        n_steps=int(raw["n_steps"]),
        batch_size=int(raw["batch_size"]),
        n_epochs=int(raw["n_epochs"]),
        learning_rate=float(raw["learning_rate"]),
        gamma=float(raw["gamma"]),
        gae_lambda=float(raw["gae_lambda"]),
        clip_range=float(raw["clip_range"]),
        ent_coef=float(raw["ent_coef"]),
        net_arch=[int(width) for width in raw["net_arch"]],
        activation=str(raw["activation"]),
    )


def check_ranges(path: Path, config: TrainConfig) -> None:
    """Settings that would only fail later, deep inside a run, fail here instead."""
    if config.ppo.activation not in ACTIVATION_NAMES:
        raise ValueError(f"{path}: ppo.activation must be one of {ACTIVATION_NAMES}")
    check_stages(path, config)
    counts = {
        "ppo.n_envs": config.ppo.n_envs,
        "curriculum.window": config.curriculum_window,
        "checkpoint.every_steps": config.checkpoint_every_steps,
        "checkpoint.eval_episodes": config.eval_episodes,
    }
    for name, value in counts.items():
        if value < 1:
            raise ValueError(f"{path}: {name} must be at least 1, not {value}")


def check_stages(path: Path, config: TrainConfig) -> None:
    """At least one known stage, none twice, and a weight above zero for each: a stage
    with weight 0 would unlock but never run, so its window would never fill and the
    stages after it would never unlock."""
    stages = config.stages
    if not stages or len(set(stages)) != len(stages):
        raise ValueError(f"{path}: stages must list at least one stage, each once")
    for stage in stages:
        if stage not in STAGE_NAMES:
            raise ValueError(f"{path}: stage {stage!r} is not one of {STAGE_NAMES}")
        weight = config.stage_weights.get(stage)
        if weight is None or not weight > 0.0:
            raise ValueError(
                f"{path}: curriculum.stage_weights needs a weight above 0 for stage {stage} "
                "(leave a stage out of `stages` instead of giving it weight 0)"
            )
