# Training guide

How to train, resume and inspect the Nine Lives policy on your own machine (ADR-003, PLAN §5.3). Training runs on the CPU; checkpoints stay in `runs/` (gitignored) on that machine.

## One-time setup (macOS, Linux)

```bash
# Python 3.11 or 3.12 (for example: pyenv install 3.12.11)
python3.12 -m venv .venv
.venv/bin/pip install -e "python[dev]"
VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop --release -m crates/nl-py/Cargo.toml
.venv/bin/pytest python/tests        # about 10 s; includes a 20k-step smoke train
```

On Linux you can install the exact CI versions instead: `.venv/bin/pip install -r python/requirements-dev.lock` (CPU-only torch), then `pip install -e python`.

Rebuild the Rust module (the `maturin develop` line) after any change under `crates/`.

**macOS with an iCloud-synced Desktop or Documents folder:** macOS marks `.pth` files there as hidden, and Python 3.12 skips hidden `.pth` files, so `import ninelives` fails outside pytest. Either run commands with `PYTHONPATH=python` (as below) or clear the flag with `chflags nohidden .venv/lib/python3.12/site-packages/*.pth`.

## Train

```bash
PYTHONPATH=python .venv/bin/python -m ninelives.train \
    --config configs/train/base.yaml --run-id stage-a-001 --max-minutes 60
```

- `--max-minutes N` saves and stops after N minutes of wall time. Without it the run goes to `total_steps` in the config.
- Ctrl-C also saves `runs/<run-id>/latest/` before exiting.
- Starting a run id that already exists is refused, so a run is never overwritten by accident.
- `--total-steps N` overrides the config's `total_steps` (the M7 full run: `--total-steps 150_000_000`).

### Settings that matter under the quiet-throttle success rule
- `curriculum.promote_on: eval` (the default) promotes a stage from a deterministic eval at each checkpoint, on fresh seeds each time. `training` judges the window of training episodes instead, and their exploration noise alone breaks the quiet rule, so it never promotes.
- `ppo.initial_action_bias: -1` and `ppo.log_std_init: 0` start each throttle's mean at the edge of zero: the deterministic policy idles at 0 while samples still fire.
- `reward.toml` `success` pays once per success, which is what rewards a quiet landing (EXPERIMENTS m7-full-002).

## Resume

```bash
PYTHONPATH=python .venv/bin/python -m ninelives.train \
    --config configs/train/base.yaml --run-id stage-a-001 --resume --max-minutes 60
```

This continues from `runs/<run-id>/latest/`: the model, the `VecNormalize` statistics, the curriculum level and its success window, and the step count. Episode seeds are re-drawn from `seed + steps`, so a resumed run does not replay old episodes. PPO on CPU is close to repeatable but not bit-exact across a resume.

## What a run writes

```
runs/<run-id>/
  config.yaml          copy of the config used
  latest/              model.zip, vecnormalize.pkl, state.json   (resume point)
  best/                the same, for the best fixed-seed eval success
  checkpoints/<steps>/ model.zip, vecnormalize.pkl every 1M steps
  tb/                  TensorBoard logs
```

## Watch it learn

```bash
.venv/bin/tensorboard --logdir runs
```

Useful scalars: `curriculum/level`, `curriculum/success_rate`, `episode/success`, `episode/crash`, `episode/saves`, `episode/air_used_kg` and `eval/success_rate` (fixed seeds at the hardest level), plus SB3's `time/fps`. On the M1 Pro (8 cores) a 64-env run does about 28k steps/s.

Scores alone can hide reward hacking: a policy can stop crashing by hovering until the time limit. Before trusting a run, watch its episodes (the browser replay arrives in M6).

## After a run

Log every run or chunk in `docs/EXPERIMENTS.md`: date, config, seed, steps, wall time, success per stage, and notes.
