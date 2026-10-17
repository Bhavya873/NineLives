"""PPO training (SPEC §9.9, §9.10; PLAN §5.3).

    python -m ninelives.train --config configs/train/base.yaml --run-id <name>
    python -m ninelives.train --config configs/train/base.yaml --run-id <name> --resume
    ... --max-minutes 60   # save and stop after an hour of wall time

Everything lands in runs/<name>/ (see run_files.py). A run always saves `latest/` when
it stops, even on Ctrl-C, so `--resume` loses at most the last rollout.
"""

import argparse
import shutil
from pathlib import Path

import torch
from stable_baselines3 import PPO
from stable_baselines3.common.utils import LinearSchedule
from stable_baselines3.common.vec_env import VecNormalize

from ninelives.callbacks import CurriculumCallback, RunCheckpointCallback, TimeLimitCallback
from ninelives.curriculum import Curriculum
from ninelives.env import NineLivesVecEnv
from ninelives.run_files import RUNS_DIR, RunState, load_checkpoint, save_checkpoint
from ninelives.sim_config import CONFIGS_DIR
from ninelives.train_config import TrainConfig, load_train_config

# Activation names a training YAML may use, and the torch layer each one means.
ACTIVATIONS = {"tanh": torch.nn.Tanh, "relu": torch.nn.ReLU}


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    """The command line (`argv`, or the real one when None)."""
    parser = argparse.ArgumentParser(description="Train the Nine Lives policy with PPO.")
    parser.add_argument("--config", type=Path, required=True, help="training YAML")
    parser.add_argument("--run-id", required=True, help="folder name under runs/")
    parser.add_argument("--resume", action="store_true", help="continue runs/<id>/latest")
    parser.add_argument("--max-minutes", type=float, default=None, help="stop after this")
    parser.add_argument("--runs-dir", type=Path, default=RUNS_DIR, help=argparse.SUPPRESS)
    return parser.parse_args(argv)


def fresh_curriculum(config: TrainConfig, env: NineLivesVecEnv) -> Curriculum:
    """Level 0 of the run's stage, with the level count the sim reports."""
    return Curriculum(
        stage=config.stage,
        level=0,
        level_count=env.sim.level_count(config.stage),
        window=config.curriculum_window,
        advance_at=config.curriculum_advance_at,
    )


def new_run(config: TrainConfig, env: NineLivesVecEnv, run_dir: Path) -> tuple[PPO, RunState]:
    """A fresh PPO model (SPEC §9.10) and run state at curriculum level 0."""
    normalizer = VecNormalize(env, norm_obs=True, norm_reward=True, clip_obs=config.clip_obs)
    ppo = config.ppo
    model = PPO(
        "MlpPolicy",
        normalizer,
        n_steps=ppo.n_steps,
        batch_size=ppo.batch_size,
        n_epochs=ppo.n_epochs,
        # Linear decay to 0 over the run (SB3 measures progress against total_steps,
        # also across a resume).
        learning_rate=LinearSchedule(ppo.learning_rate, 0.0, 1.0),
        gamma=ppo.gamma,
        gae_lambda=ppo.gae_lambda,
        clip_range=ppo.clip_range,
        ent_coef=ppo.ent_coef,
        policy_kwargs={
            "net_arch": {"pi": ppo.net_arch, "vf": ppo.net_arch},
            "activation_fn": ACTIVATIONS[ppo.activation],
        },
        tensorboard_log=str(run_dir / "tb"),
        seed=config.seed,
        device="cpu",
    )
    curriculum = fresh_curriculum(config, env)
    state = RunState(
        steps=0,
        total_steps=config.total_steps,
        curriculum=curriculum,
        best_eval_success=-1.0,
        wall_seconds=0.0,
    )
    return model, state


def resume_run(config: TrainConfig, env: NineLivesVecEnv, run_dir: Path) -> tuple[PPO, RunState]:
    """The model and state saved in runs/<id>/latest, with fresh seeds so the resumed
    run does not replay the episodes it has already seen."""
    model, _, state = load_checkpoint(run_dir / "latest", env, fresh_curriculum(config, env))
    model.set_random_seed(config.seed + state.steps)
    if state.total_steps and state.total_steps != config.total_steps:
        print(
            f"warning: this run was planned for {state.total_steps} steps, the config now says "
            f"{config.total_steps}; the learning rate schedule will jump"
        )
    state.total_steps = config.total_steps
    return model, state


def train(argv: list[str] | None = None) -> RunState:
    """Runs (or resumes) training until total_steps or the time limit; returns the state."""
    args = parse_args(argv)
    config = load_train_config(args.config)
    run_dir = args.runs_dir / args.run_id
    if not args.resume and (run_dir / "latest").exists():
        raise SystemExit(f"{run_dir} already has a run; pass --resume or pick a new --run-id")
    env = NineLivesVecEnv(
        config.ppo.n_envs, config.seed, config.mode, config.stage, config.randomize
    )
    if args.resume:
        model, state = resume_run(config, env, run_dir)
    else:
        run_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy(args.config, run_dir / "config.yaml")
        # The TOML configs too: the reward and scenarios are part of what was trained.
        shutil.copytree(CONFIGS_DIR, run_dir / "configs", ignore=shutil.ignore_patterns("train"))
        model, state = new_run(config, env, run_dir)
    # Before learn(): it resets every env, and those first episodes must already run
    # at the curriculum's level.
    env.set_stage_and_level(state.curriculum.stage, state.curriculum.level)
    checkpoints = RunCheckpointCallback(state, run_dir, config)
    callbacks = [CurriculumCallback(state), checkpoints, TimeLimitCallback(args.max_minutes)]
    try:
        model.learn(
            total_timesteps=max(config.total_steps - state.steps, 0),
            callback=callbacks,
            reset_num_timesteps=not args.resume,
            tb_log_name="ppo",
        )
    except KeyboardInterrupt:
        print("stopped by Ctrl-C")
    # Saved on a normal end, the time limit or Ctrl-C, but not after an error: a
    # diverged model must not overwrite the last good latest/.
    checkpoints.record_progress(model.num_timesteps)
    normalizer = model.get_vec_normalize_env()
    if normalizer is not None:
        save_checkpoint(run_dir / "latest", model, normalizer, state)
    print(f"saved {run_dir / 'latest'} at {state.steps} steps")
    return state


if __name__ == "__main__":
    train()
