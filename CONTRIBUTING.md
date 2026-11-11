# Contributing to Nine Lives

Nine Lives is a browser driving game where an AI fires a car's 10 cold-gas thrusters automatically, so the car always lands on its wheels. One Rust physics sim (Rapier) is used for both RL training (Python, PPO) and the game (wasm + three.js).

Read [`SPEC.md`](SPEC.md) first. It is the source of truth for what the project builds. This file covers how to work on it. Decisions and experiment logs live in [`docs/DECISIONS.md`](docs/DECISIONS.md) and [`docs/EXPERIMENTS.md`](docs/EXPERIMENTS.md).

## Layout

- `crates/nl-sim` holds the world, vehicle, thrusters, tank, modes, courses, events, obs, reward, scenarios and driver bot.
- `crates/nl-control` holds the Controller trait, Off, Manual, Pd and thruster allocation.
- `crates/nl-policy` loads the policy JSON and runs the MLP forward pass.
- `crates/nl-py` is the PyO3 module `ninelives_sim` (BatchedSim).
- `crates/nl-wasm` is the wasm-bindgen `Game` API.
- `python/ninelives` holds the Gymnasium/SB3 env, train, eval, export and charts.
- `web/` is the Vite + TypeScript + three.js game.
- `configs/` holds the TOML configs and `configs/train/*.yaml` PPO settings.
- `courses/` holds the course JSON.
- `docs/` holds the decision log, experiment log and design notes.

## Golden rules

1. **Physics lives only in `crates/nl-sim`.** Thruster forces, the air tank, events, observations and rewards are never re-implemented in Python or TypeScript. Python and the browser call into Rust.
2. **Determinism.** Rapier runs with `enhanced-determinism`. Use only a seeded `ChaCha8Rng`; no system time, `thread_rng` or `HashMap` iteration in sim logic. The golden-trace test must pass on native and wasm.
3. **Conventions** (SPEC section 4): SI units; world +Y up; car body frame +X forward, +Y up, +Z right; `dt = 1/120`, control at 60 Hz; nozzle order `L_FL L_FR L_RL L_RR S_FL S_RL S_FR S_RR B_L B_R`; wheel order `FL FR RL RR`.
4. **The observation layout** is defined once in Rust (`ObsSpec`, 41 values in v1). Any change bumps the schema hash. Python, the exporter and the browser all check the hash.
5. **Write the sign test first.** When you add or change a force, torque or frame transform, write the unit test that pins its sign before the code.
6. **Configs, not constants.** Tunable numbers go in `configs/*.toml` (vehicle, modes, reward, scenarios). Code holds no magic numbers that the spec marks *(start)*.
7. **No branding.** Do not use real car-maker or space-company names, logos or car shapes in code, assets or UI.
8. **Do not commit** training checkpoints (`runs/`), large binaries or secrets. `models/policy.json` and `results/*.json|csv|svg` are committed. Training runs locally, so checkpoints stay in `runs/` on the training machine (ADR-003); the only committed checkpoints are the 8 or fewer viewer exports under `web/public/checkpoints/` (SPEC section 9.12).

## Commands

```bash
# Rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p nl-sim --example drive
cargo run --release -p nl-control --example eval -- --controller pd --stage A --mode realistic --episodes 1000 --seed 0

# Python (venv with Python 3.11 or 3.12)
pip install -e python[dev]
maturin develop --release -m crates/nl-py/Cargo.toml
ruff check python && pytest python/tests
python -m ninelives.train --config configs/train/base.yaml --run-id <name>
python -m ninelives.eval --controller ai --stage A --mode realistic --episodes 1000 --seed 0
python -m ninelives.export --run runs/<name> --out models/policy.json
python -m ninelives.charts

# Web
cd web && npm install
npm run wasm        # builds crates/nl-wasm into web/src/wasm (release: + wasm-opt)
npm run test:wasm   # Node tests: wasm golden trace + Game API
node scripts/measure-tick.mjs   # after npm run wasm:release: ms per tick
npm run dev         # local game
npm run lint && npm run build
npx playwright test
```

## Checks before you call something done

- `cargo fmt`, `clippy -D warnings` and `cargo test` are clean.
- If you touched Python: `ruff` and `pytest` pass, including the export parity test.
- If you touched web or wasm: `npm run lint`, `npm run build`, the wasm golden-trace test and Playwright pass.
- Log any training run in `docs/EXPERIMENTS.md`: date, config, seed, steps, wall time, results, notes.
- For RL changes, watch a clip or the browser replay, not just the score. Reward hacking shows up in behaviour first.

## Style

- **Rust:** stable toolchain, `rustfmt` defaults, no `unwrap()` in library code outside tests (use `Result` with `thiserror`), `f32` in sim code.
- **Python:** type hints, `ruff`, small modules, no physics code.
- **TypeScript:** strict mode, ESLint + Prettier, no framework, rendering separated from input and HUD.
- Keep functions small and named for what they do.

### Readability

Readable beats clever, everywhere. A new teammate should follow any file without help.

- **Names say what and in what unit:** `speed_m_s`, `air_left_kg`, `tilt_rad`. One-letter names only for loop indices or maths copied from a SPEC formula.
- **Small pieces:** a function does one thing and fits on a screen (about 40 lines). A file covers one concept (`tank.rs`, `valve_lag.rs`).
- **Plain constructs:** structs, enums, `for` loops and `match`. No home-made macros, no generic or trait-object gymnastics, no iterator chains longer than about three steps, no lifetimes beyond the obvious. Traits only where SPEC asks for one (`Controller`).
- **Comments:** every public item has a `///` (or docstring/JSDoc) comment saying what it is and its units. Inline comments explain why. A formula cites its SPEC section, e.g. `// SPEC §7.5: mass flow = force / exhaust speed`.
- **Module header:** every file starts with 2-4 lines saying what it does and how it fits in.
- **Errors** tell a person what to fix: "course.json: piece 4 'ramp' is missing 'length'".
- **Tests read like examples:** `firing_front_left_lift_raises_left_front_corner`, arranged as arrange / act / assert.
- **Python:** functions over classes unless a class is needed (Gymnasium/SB3 interfaces). **TypeScript:** plain functions and modules; keep three.js setup apart from game logic.

## Gotchas

- Rapier's vehicle and query APIs change between releases. Read the docs for the pinned version (`DynamicRayCastVehicleController::update_vehicle` takes the query pipeline).
- Raycast wheel rays point "down" in the body frame, so on an inverted car they point up. Count wheel contact only when the car is upright enough (SPEC section 7.3).
- SB3 expects actions in [-1, 1]; the sim maps them to throttles in [0, 1].
- Export VecNormalize statistics with the policy, or the browser policy will act on unnormalized inputs.
- Wrap all `localStorage` access in try/catch.

## Branches and pull requests

Use one short-lived branch per small feature, off `main`, and one pull request per branch. Aim for under 400 changed lines, excluding lockfiles. A pull request description says what and why, how to test it, which SPEC section it covers, and includes a screenshot or GIF for anything visual.
