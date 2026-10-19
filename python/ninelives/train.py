"""PPO training (SPEC §9.9, §9.10; PLAN §5.3).

    python -m ninelives.train --config configs/train/base.yaml --run-id <name>
    python -m ninelives.train --config configs/train/base.yaml --run-id <name> --resume
    ... --max-minutes 60   # save and stop after an hour of wall time

Everything lands in runs/<name>/ (see run_files.py). A run always saves `latest/` when
it stops, even on Ctrl-C, so `--resume` loses at most the last rollout.
"""

import argparse
import json
import pickle
import shutil
from pathlib import Path

import torch
from stable_baselines3 import PPO
from stable_baselines3.common.utils import LinearSchedule
from stable_baselines3.common.vec_env import VecNormalize

from ninelives.callbacks import CurriculumCallback, RunCheckpointCallback, TimeLimitCallback
from ninelives.curriculum import Curriculum
from ninelives.env import NineLivesVecEnv
from ninelives.eval import evaluate
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
    parser.add_argument(
        "--total-steps", type=int, default=None, help="override the config's total_steps"
    )
    parser.add_argument(
        "--init-from",
        type=Path,
        default=None,
        help="checkpoint folder (e.g. runs/stage-a-004/best) whose policy a new run starts from",
    )
    parser.add_argument(
        "--keep-curriculum",
        action="store_true",
        help="with --init-from: also carry over that checkpoint's unlocked stages and levels",
    )
    parser.add_argument("--runs-dir", type=Path, default=RUNS_DIR, help=argparse.SUPPRESS)
    return parser.parse_args(argv)


def fresh_curriculum(config: TrainConfig, env: NineLivesVecEnv) -> Curriculum:
    """The run's first stage unlocked at level 0, with the level counts the sim reports."""
    return Curriculum(
        stages=config.stages,
        weights=config.stage_weights,
        level_counts={stage: env.sim.level_count(stage) for stage in config.stages},
        window=config.curriculum_window,
        advance_at=config.curriculum_advance_at,
        unlock_every_steps=config.curriculum_unlock_every_steps,
    )


def pd_baseline(config: TrainConfig) -> dict[str, float]:
    """PD's success per stage at full difficulty on the validation seeds: the bar the
    newest stage must match to unlock the next (`curriculum.unlock.beat_pd`)."""
    rates = {}
    for stage in config.stages:
        report = evaluate("pd", stage, config.mode, config.eval_episodes, config.eval_seed)
        rates[stage] = float(report["summary"]["success_rate"])
    print(f"PD baseline on the validation seeds: {rates}")
    return rates


def warm_start(model: PPO, normalizer: VecNormalize, checkpoint: Path) -> None:
    """Starts `model` from the policy in `checkpoint` (same network shape) and its
    observation statistics, which the policy was trained to read. Reward statistics
    stay fresh, since the reward may have changed since that run."""
    old = PPO.load(checkpoint / "model.zip", device="cpu")
    model.policy.load_state_dict(old.policy.state_dict())
    with (checkpoint / "vecnormalize.pkl").open("rb") as file:
        normalizer.obs_rms = pickle.load(file).obs_rms
    print(f"warm start from {checkpoint}")


def start_with_action_bias(model: PPO, action_bias: float) -> None:
    """Sets a fresh policy's action-mean bias. At −1 (SB3 clips actions to [−1, 1], and
    the sim maps −1 to throttle 0) the deterministic policy starts idling at zero
    throttle, as the quiet-throttle success rule (SPEC §9.5) needs, rather than at half
    throttle on every nozzle; sampled actions still fire about half the time, so firing
    gets explored (docs/EXPERIMENTS.md, m7-full runs)."""
    with torch.no_grad():
        model.policy.action_net.bias.fill_(action_bias)


def new_run(
    config: TrainConfig,
    env: NineLivesVecEnv,
    run_dir: Path,
    init_from: Path | None = None,
    keep_curriculum: bool = False,
) -> tuple[PPO, RunState]:
    """A new PPO model (SPEC §9.10), fresh or warm-started from `init_from`, and run state
    at curriculum level 0."""
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
            "log_std_init": ppo.log_std_init,
        },
        tensorboard_log=str(run_dir / "tb"),
        seed=config.seed,
        device="cpu",
    )
    if init_from is None:
        start_with_action_bias(model, ppo.initial_action_bias)
    else:
        warm_start(model, normalizer, init_from)
    curriculum = fresh_curriculum(config, env)
    if init_from is not None and keep_curriculum:
        saved = json.loads((init_from / "state.json").read_text())["curriculum"]
        curriculum.restore(saved)
        curriculum.last_unlock_step = 0
        env.env_method("set_stage_mix", curriculum.mix())
        print(f"curriculum carried over from {init_from}: {curriculum.mix()}")
    if config.curriculum_promote_on == "eval" and config.curriculum_unlock_on_pd:
        curriculum.pd_success = pd_baseline(config)
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
    if args.total_steps is not None:
        config.total_steps = args.total_steps
    run_dir = args.runs_dir / args.run_id
    if not args.resume and (run_dir / "latest").exists():
        raise SystemExit(f"{run_dir} already has a run; pass --resume or pick a new --run-id")
    env = NineLivesVecEnv(
        config.ppo.n_envs,
        config.seed,
        config.mode,
        config.stages[0],
        config.randomize,
        config.reward_overrides,
    )
    if args.resume:
        model, state = resume_run(config, env, run_dir)
    else:
        run_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy(args.config, run_dir / "config.yaml")
        # The TOML configs too: the reward and scenarios are part of what was trained.
        shutil.copytree(CONFIGS_DIR, run_dir / "configs", ignore=shutil.ignore_patterns("train"))
        model, state = new_run(config, env, run_dir, args.init_from, args.keep_curriculum)
    # Before learn(): it resets every env, and those first episodes must already run
    # the curriculum's stages and levels.
    env.set_stage_mix(state.curriculum.mix())
    checkpoints = RunCheckpointCallback(state, run_dir, config)
    promote_from_training = config.curriculum_promote_on == "training"
    callbacks = [
        CurriculumCallback(state, promote_from_training),
        checkpoints,
        TimeLimitCallback(args.max_minutes),
    ]
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
