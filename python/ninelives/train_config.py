"""Training run settings, read from `configs/train/*.yaml` (SPEC §9.10)."""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

# Activation names `ppo.activation` may use (train.py maps them to torch layers).
ACTIVATION_NAMES = ("tanh", "relu")
# The scenario stages the sim knows (SPEC §9.6).
STAGE_NAMES = ("A", "B", "C", "D", "E", "F")


# The final eval (results/) uses seeds 0–999; training's validation seeds must stay above.
TEST_SEED_COUNT = 1000


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
    # Starting spread of the policy's action noise (SB3 `log_std_init`; 0 means std 1).
    log_std_init: float
    # Starting bias of the policy's action mean; −1 or below starts every throttle at 0.
    initial_action_bias: float


@dataclass
class ResidualSettings:
    """Residual control (M7 approach 2): each throttle is the base controller's plus
    `scale` × the policy's action, clamped to [0, 1], all in Rust."""

    # The controller the policy nudges; only "pd".
    base: str
    # Throttle per unit of action: an action in [−1, 1] moves a throttle by up to ±scale.
    scale: float
    # Divide each nudge by the mode's thrust scale, as PD's allocation does (round 6).
    force_scaled: bool = False
    # An 11th action gates every nudge by (a + 1) / 2 (round 6).
    gated: bool = False

    def as_sim_json(self) -> dict[str, object]:
        """The `residual` entry of the sim config JSON (crates/nl-py/src/config.rs)."""
        return {
            "base": self.base,
            "scale": self.scale,
            "force_scaled": self.force_scaled,
            "gated": self.gated,
        }


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
    # False keeps `mode`'s preset under randomization: a policy for one mode (round 7).
    randomize_mode: bool
    total_steps: int
    ppo: PpoSettings
    clip_obs: float
    curriculum_window: int
    # Bar to move up a level: one value, or one per level (the last repeats).
    curriculum_advance_at: float | list[float]
    # Eval mode: unlock the next stage once the newest one matches PD at its hardest level.
    curriculum_unlock_on_pd: bool
    # Eval mode: also unlock the next stage after this many steps without one (None: never).
    curriculum_unlock_every_steps: int | None
    # Reward weights replaced for this run, e.g. {"hard_landing": 0.0} for an ablation.
    reward_overrides: dict[str, float]
    # What promotes a stage: "eval" (a deterministic fixed-seed eval at each checkpoint)
    # or "training" (the window of training episodes, which include exploration noise).
    curriculum_promote_on: str
    checkpoint_every_steps: int
    eval_episodes: int
    eval_seed: int
    # Residual control around PD; None trains the policy's action as the throttle.
    residual: ResidualSettings | None = None
    # Start with every stage unlocked at its hardest level (no curriculum to climb).
    curriculum_start_at_hardest: bool = False


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
            randomize_mode=strict_bool("randomize_mode", raw.get("randomize_mode", True)),
            total_steps=int(raw["total_steps"]),
            ppo=read_ppo(raw["ppo"]),
            clip_obs=float(raw["normalize"]["clip_obs"]),
            curriculum_window=int(raw["curriculum"]["window"]),
            curriculum_advance_at=read_bar(raw["curriculum"]["advance_at"]),
            curriculum_unlock_on_pd=strict_bool(
                "curriculum.unlock.beat_pd",
                raw["curriculum"].get("unlock", {}).get("beat_pd", False),
            ),
            curriculum_unlock_every_steps=optional_int(
                raw["curriculum"].get("unlock", {}).get("every_steps")
            ),
            reward_overrides={
                str(name): float(value) for name, value in raw.get("reward_overrides", {}).items()
            },
            curriculum_promote_on=str(raw["curriculum"].get("promote_on", "eval")),
            checkpoint_every_steps=int(raw["checkpoint"]["every_steps"]),
            eval_episodes=int(raw["checkpoint"]["eval_episodes"]),
            eval_seed=int(raw["checkpoint"]["eval_seed"]),
            residual=read_residual(raw.get("residual")),
            curriculum_start_at_hardest=strict_bool(
                "curriculum.start_at_hardest", raw["curriculum"].get("start_at_hardest", False)
            ),
        )
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{path}: missing or invalid setting: {error}") from error
    check_ranges(path, config)
    return config


def read_residual(raw: dict[str, Any] | None) -> ResidualSettings | None:
    """The optional `residual` section: `{base: pd, scale: 0.5}`, optionally with
    `force_scaled` and `gated` (both default false)."""
    if raw is None:
        return None
    unknown = set(raw) - set(ResidualSettings.__dataclass_fields__)
    if unknown:
        raise ValueError(f"unknown residual settings {sorted(unknown)}")
    return ResidualSettings(
        base=str(raw["base"]),
        scale=float(raw["scale"]),
        force_scaled=strict_bool("residual.force_scaled", raw.get("force_scaled", False)),
        gated=strict_bool("residual.gated", raw.get("gated", False)),
    )


def read_bar(value: object) -> float | list[float]:
    """`curriculum.advance_at`: one number, or a list of one number per level."""
    if isinstance(value, list):
        return [float(item) for item in value]
    return float(value)  # type: ignore[arg-type]


def optional_int(value: object) -> int | None:
    """An integer setting that may be left out (None)."""
    return None if value is None else int(value)  # type: ignore[call-overload]


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
        # Older run configs lack these; SB3's own defaults keep their behaviour.
        log_std_init=float(raw.get("log_std_init", 0.0)),
        initial_action_bias=float(raw.get("initial_action_bias", 0.0)),
    )


def check_ranges(path: Path, config: TrainConfig) -> None:
    """Settings that would only fail later, deep inside a run, fail here instead."""
    if config.ppo.activation not in ACTIVATION_NAMES:
        raise ValueError(f"{path}: ppo.activation must be one of {ACTIVATION_NAMES}")
    check_stages(path, config)
    if config.eval_seed < TEST_SEED_COUNT:
        raise ValueError(
            f"{path}: checkpoint.eval_seed must be at least {TEST_SEED_COUNT}: seeds "
            f"0–{TEST_SEED_COUNT - 1} are the test seeds of the final eval, never used to "
            "choose checkpoints or promote stages"
        )
    check_residual(path, config)
    if config.curriculum_promote_on not in ("eval", "training"):
        raise ValueError(f'{path}: curriculum.promote_on must be "eval" or "training"')
    counts = {
        "ppo.n_envs": config.ppo.n_envs,
        "curriculum.window": config.curriculum_window,
        "checkpoint.every_steps": config.checkpoint_every_steps,
        "checkpoint.eval_episodes": config.eval_episodes,
    }
    for name, value in counts.items():
        if value < 1:
            raise ValueError(f"{path}: {name} must be at least 1, not {value}")


def check_residual(path: Path, config: TrainConfig) -> None:
    """A residual run nudges PD, so its policy must start at a zero action: the action
    head starts zeroed, and an action bias would be silently ignored."""
    residual = config.residual
    if residual is None:
        return
    if residual.base != "pd":
        raise ValueError(f'{path}: residual.base must be "pd", not {residual.base!r}')
    if not residual.scale > 0.0:
        raise ValueError(f"{path}: residual.scale must be above 0, not {residual.scale}")
    if config.ppo.initial_action_bias != 0.0:
        raise ValueError(
            f"{path}: with residual control set ppo.initial_action_bias to 0 (a zero action "
            "is exactly PD); a bias of −1 would start by pulling every throttle down"
        )


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
