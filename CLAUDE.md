# Nine Lives

A browser driving game. An AI fires the car's 10 cold-gas thrusters so it lands on its wheels. One Rust physics sim (Rapier) is used for both training (Python, PPO) and the game (wasm + three.js). `SPEC.md` is the source of truth for what to build; `docs/README.md` is the index of everything else. The Gauntlet course is retired and kept for history only.

## Golden rules
1. Physics lives only in `crates/nl-sim`. Thruster forces, the air tank, events, obs and reward are never re-implemented in Python or TypeScript; they call into Rust.
2. Deterministic: Rapier with `enhanced-determinism`, seeded `ChaCha8Rng` only, no system time, `thread_rng` or `HashMap` iteration in sim logic. The golden trace (`crates/nl-sim/tests/golden/flat_drive.json`, hash `72959553d1b9e680`) must pass on native and wasm, and must not change unless you add a row to `docs/DECISIONS.md` saying why.
3. Conventions (SPEC section 4): SI units, world +Y up, body frame +X forward / +Y up / +Z right, `dt = 1/120`, control at 60 Hz.
4. The observation layout is defined once in Rust (`ObsSpec`, 43 values in v2). Any change bumps the schema hash; Python, the exporter and the browser all check it.
5. Sign tests first: when adding or changing a force, torque or frame transform, write the test that pins its sign before the code.
6. Configs, not constants: tunable numbers live in `configs/*.toml`, not in code.
7. No branding: never use Tesla or SpaceX names, logos or shapes.
8. Never commit `runs/` checkpoints, large binaries or secrets.

Also: every number in docs or README comes from a file in `results/` (see `results/README.md`); never invent measurements. If the spec is wrong or unclear, pick the simplest option, update `SPEC.md`, and add one row to `docs/DECISIONS.md`. Log every training run in `docs/EXPERIMENTS.md`.

## Layout
- `crates/nl-sim` sim core, `nl-control` controllers (Off, Manual, PD, Hybrid), `nl-policy` MLP runner, `nl-py` PyO3 batched sim, `nl-wasm` browser API.
- `python/ninelives` env, train, eval, export, charts. `web/` the Vite + three.js game.
- `configs/` TOML settings and `configs/train/` PPO yaml. `courses/` course JSON. `models/policy.json` the shipped policy. `results/` eval outputs. `docs/` notes, contracts and decisions.
- `deploy/` and `Dockerfile` build the static site (nginx); `railway.json` points Railway at it.

## Commands
```bash
# Rust
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Python (venv, Python 3.11 or 3.12)
pip install -e python[dev]
maturin develop --release -m crates/nl-py/Cargo.toml
ruff check python && pytest python/tests

# Web (Node 22)
cd web && npm ci
npm run wasm && npm run test:wasm && npm run lint && npm run build
npm run test:e2e
```

## Before calling anything done
- fmt, clippy and tests are clean; ruff and pytest pass if Python changed; lint, wasm tests, build and e2e pass if web or wasm changed.
- For RL changes, watch a replay, not just the score. Reward hacking shows up in behavior first.

## Style
- Plain, readable code: descriptive names with units (`speed_m_s`), small functions, short lowercase comments that say why, a header on each file saying what it does.
- Rust: no `unwrap()` in library code. Python: type hints, ruff, no physics. TypeScript: strict, no framework.
- One branch per change, named `claude/<name>`, one PR into `main`. The owner reviews and merges.

## Gotchas
- Rapier's vehicle and query APIs change between releases; read the docs for the pinned version.
- Wheel rays point "down" in the body frame, so on an inverted car they point up. Count wheel contact only when the car is upright enough (SPEC 7.3).
- SB3 actions are in [-1, 1]; the sim maps them to throttles in [0, 1].
- Export VecNormalize statistics with the policy, or the browser acts on unnormalized inputs.
- Wrap all `localStorage` access in try/catch.
