# Nine Lives

A browser driving game where an AI fires a car's 10 cold-gas thrusters automatically, so the car always lands on its wheels. One Rust physics sim (Rapier) is used for both RL training (Python, PPO) and the game (wasm + three.js).

**Read `SPEC.md` before starting any work.** It is the source of truth for what to build. This file is how to work.

## How to work through the project
- Build the milestones in `SPEC.md` §13 in order (M0 → M8). Do not start a milestone until the previous one meets every acceptance criterion.
- At the start of a session, find the current milestone: check `docs/DECISIONS.md`, `docs/EXPERIMENTS.md` and the milestone checkboxes.
- Tick a checkbox in `SPEC.md` §13 only when its tests pass in CI or locally.
- Before a large change, write a short plan. After it, run the full checks (below).
- `docs/PLAN.md` holds the agreed build plan, review gates and training approach. Stop for the owner's review after M4 and after M7.
- If the spec is wrong or unclear, pick the simplest option that fits the spec's intent, implement it, update `SPEC.md`, and add one line to `docs/DECISIONS.md` (date, decision, reason). Ask the owner only when a choice is hard to reverse.

## Golden rules
1. **Physics lives only in `crates/nl-sim`.** Thruster forces, the air tank, events, observations and rewards are never re-implemented in Python or TypeScript. Python and the browser call into Rust.
2. **Determinism.** Rapier runs with `enhanced-determinism`. Use only seeded `ChaCha8Rng`; no system time, `thread_rng` or `HashMap` iteration in sim logic. The golden-trace test must pass on native and wasm.
3. **Conventions** (SPEC §4): SI units; world +Y up; car body frame +X forward, +Y up, +Z right; `dt = 1/120`, control at 60 Hz; nozzle order `L_FL L_FR L_RL L_RR S_FL S_RL S_FR S_RR B_L B_R`; wheel order `FL FR RL RR`.
4. **Observation layout** is defined once in Rust (`ObsSpec`, 41 values in v1). Any change bumps the schema hash. Python, the exporter and the browser all check the hash.
5. **Sign tests first.** When adding or changing a force, torque or frame transform, write the unit test that pins its sign before the code.
6. **Configs, not constants.** Tunable numbers go in `configs/*.toml` (vehicle, modes, reward, scenarios). Code holds no magic numbers that the spec marks *(start)*.
7. **No branding.** Never use Tesla or SpaceX names, logos or car shapes in code, assets or UI. Use "inspired by" wording in docs only.
8. **Never commit** training checkpoints (`runs/`), large binaries or secrets to `main` or a feature branch. `models/policy.json` and `results/*.json|csv|svg` are committed. Training runs locally, so checkpoints stay in `runs/` on the training machine (ADR-003); the only committed checkpoints are the ≤ 8 viewer exports under `web/public/checkpoints/` (SPEC §9.12).

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
- `docs/` holds DECISIONS.md and EXPERIMENTS.md.

## Git and pull requests
- One short-lived branch per small feature, named `claude/<feature>`, off `main`. One PR per branch into `main`; aim for < 400 changed lines excluding lockfiles.
- Commits are authored as `Bhavya Modi <51858604+Bhavya873@users.noreply.github.com>` and end with the `Co-Authored-By: Claude` trailer. The owner reviews and merges every PR.
- Before opening a PR: run `/simplify`, then `/code-review`, then `/security-review` if the change parses external input, then the full checks below.
- A PR description says what and why, how to test it, which SPEC section it covers, and includes a screenshot or GIF for anything visual.

## Commands
(Create these in M0; keep this list accurate as scripts change.)
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

## Checks before calling anything done
- `cargo fmt`, `clippy -D warnings` and `cargo test` are clean.
- If Python was touched: `ruff` and `pytest` pass, including the export parity test.
- If web or wasm was touched: `npm run lint`, `npm run build`, the wasm golden-trace test and Playwright pass.
- Any training run is logged in `docs/EXPERIMENTS.md`: date, config, seed, steps, wall time, results, notes.
- For RL changes, watch a clip or the browser replay, not just the score. Reward hacking shows up in behaviour first.

## Style
- **Rust:** stable toolchain, `rustfmt` defaults, no `unwrap()` in library code outside tests (use `Result` with `thiserror`), `f32` in sim code.
- **Python:** type hints, `ruff`, small modules, no physics code.
- **TypeScript:** strict mode, ESLint + Prettier, no framework, rendering separated from input and HUD.
- Keep functions small and named for what they do.

### Readability: write it like a careful junior developer
Readable beats clever, everywhere. A new teammate should follow any file without help.
- **Names say what and in what unit:** `speed_m_s`, `air_left_kg`, `tilt_rad`. One-letter names only for loop indices or maths copied from a SPEC formula.
- **Small pieces:** a function does one thing and fits on a screen (about 40 lines). A file covers one concept (`tank.rs`, `valve_lag.rs`).
- **Plain constructs:** structs, enums, `for` loops and `match`. No home-made macros, no generic or trait-object gymnastics, no iterator chains longer than about three steps, no lifetimes beyond the obvious. Traits only where SPEC asks for one (`Controller`).
- **Comments:** every public item has a `///` (or docstring/JSDoc) comment saying what it is and its units. Inline comments explain **why**. A formula cites its SPEC section, e.g. `// SPEC §7.5: mass flow = force / exhaust speed`.
- **Module header:** every file starts with 2–4 lines saying what it does and how it fits in.
- **Errors** tell a person what to fix: "course.json: piece 4 'ramp' is missing 'length'".
- **Tests read like examples:** `firing_front_left_lift_raises_left_front_corner`, arranged as arrange / act / assert.
- **Python:** functions over classes unless a class is needed (Gymnasium/SB3 interfaces). **TypeScript:** plain functions and modules; three.js setup kept apart from game logic.

## Gotchas
- Rapier's vehicle and query APIs change between releases. Read the docs for the pinned version (`DynamicRayCastVehicleController::update_vehicle` takes the query pipeline).
- Raycast wheel rays point "down" in the body frame, so on an inverted car they point up. Count wheel contact only when the car is upright enough (SPEC §7.3).
- SB3 expects actions in [−1, 1]; the sim maps them to throttles in [0, 1].
- Export VecNormalize statistics with the policy, or the browser policy will act on unnormalized inputs.
- Wrap all `localStorage` access in try/catch.
