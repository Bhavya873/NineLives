"""Training run settings, read from `configs/train/*.yaml` (SPEC §9.10)."""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

# Activation names `ppo.activation` may use (train.py maps them to torch layers).
ACTIVATION_NAMES = ("tanh", "relu")


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
    stage: str
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
            stage=str(raw["stage"]),
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
    counts = {
        "ppo.n_envs": config.ppo.n_envs,
        "curriculum.window": config.curriculum_window,
        "checkpoint.every_steps": config.checkpoint_every_steps,
        "checkpoint.eval_episodes": config.eval_episodes,
    }
    for name, value in counts.items():
        if value < 1:
            raise ValueError(f"{path}: {name} must be at least 1, not {value}")
