# Nine Lives — Build Plan and Deliverables

Status: **approved 2026-10-17** · Based on `SPEC.md` (v1, 2026-10-16), `CLAUDE.md` and the owner's answers of 2026-10-17

This plan turns the spec into an ordered build. It doesn't restate the spec. Section numbers like §7.4 point into `SPEC.md`. The owner approved it on 2026-10-17.

---

## 1. Summary

| | |
|---|---|
| **What ships** | A static browser game (Rust → wasm + three.js) where a learned policy fires 10 cold-gas thrusters so the car lands on its wheels. A hard or upside-down landing makes the car explode and respawn. It also ships a reproducible Off / PD / AI evaluation with charts and a written-up reward-hacking experiment (the hover hack). |
| **Core rule** | All physics lives in one Rust crate (`nl-sim`). Python training and the browser call into it, and a golden trace checks determinism on native and wasm. |
| **Build order** | M0 → M8 in order (§13), with full spec scope and no external deadline. M9 is optional. |
| **Training** | PPO on CPU (no GPU needed), run as resumable 1-hour chunks in this cloud environment, with your machine as a fallback (§5). |
| **Biggest risks** | PPO not converging on the later stages, CPU training time, and native-vs-wasm determinism. |

---

## 2. Owner decisions (2026-10-17)

| # | Decision | Effect on the plan |
|---|---|---|
| D1 | **A hard or upside-down landing explodes the car, and it respawns.** | Crash rules change (§3.1). The roof-recovery problem goes away: a roof landing is a crash by design. Stage B shrinks to slow side tip-overs. |
| D2 | **No external platform requirements.** Train with our own setup. | §5 covers how training runs. No third-party AI services, no backend. The spec's "no server" non-goal stands. |
| D3 | **Stop for review** after M4 (before training starts) and after M7 (before polish). | Built into §4. |
| D4 | **Small-feature PRs, authored by the owner, reviewed by the owner.** | §6. The GitHub connection acts as `Bhavya873`, so PRs show you as the author. Commits are authored as Bhavya Modi, with a `Co-Authored-By: Claude` trailer. |
| D5 | **Code a junior developer could have written and can read.** | New coding standard (§7). `CLAUDE.md` gets updated to match. |
| D6 | **Watch the cars learn.** Training stays PPO (many headless cars feed one shared policy), plus a *training viewer*: a crowd of cars (about 64) runs each saved checkpoint through an obstacle arena, so you see them improve round by round. | M5 exports a few evenly spaced checkpoints as policy JSON; M6 adds the viewer on Stage A drops; M7's obstacle arena course gives it ramps and blocks. |
| D7 | **Lab-arena look everywhere:** checkered floor, neutral lighting, plain-coloured ramps and blocks, like a physics simulator's test arena. | Done for the floor, light and backdrop after M3; course pieces use plain colours in M7. |

---

## 3. Spec changes this plan makes
Each change gets a `SPEC.md` edit and a line in `docs/DECISIONS.md` in M0.

### 3.1 Crash, explosion and respawn (replaces §7.9 "Crash")
- **Hard impact:** any chassis or roof contact with relative normal speed > 6 m/s → crash. *(unchanged)*
- **Upside down:** **any** roof contact while tilt > 120° → crash, at any speed. *(was: also speed > 3 m/s)*
- **Stuck:** tilt > 100° for > 2 s → crash. *(was 5 s; nothing can recover from there anyway)*
- **Out of bounds:** unchanged.
- **In the game:** on `Crash`, freeze the car's physics body, then play the **explosion**: a flash, a shockwave ring, about 20 low-poly debris pieces, and a camera shake (none with reduced motion). After **1.5 s**, **respawn** at the last checkpoint (pose and velocities; see DECISIONS 2026-10-17, no `restore`) with a full tank, and add 1 to the crash counter. The HUD shows "Lives used: 3", which ties into the Nine Lives name.
- **In training:** a crash is still terminal with −10 reward. Respawn happens only in the game.
- **Stage B** becomes "tipped onto its side at low speed" (roll 60–110°, recover without touching the roof).
- Tests: an upside-down 0.5 m drop crashes; an upright 2 m drop doesn't; respawn puts the car at the checkpoint pose with a full tank (see `tests/respawn.rs`).

### 3.2 Smaller defaults (unless the owner objects)
- `car_row` boxes are 4.5 × h × 1.9 m with 0.6 m gaps. Stage C uses rows of 1–3 cars, because in Realistic a plain Hop clears about 13 m and 5 cars need about 25 m. The Gauntlet's 5-car row gets a run-up ramp.
- The save *event* holds for 0.5 s; Stage A/B *success* holds for 1.0 s. These are two separate timers.
- Air mass isn't added to the vehicle mass in v1.
- Versions are pinned at scaffold time after checking the registries. The spec names rapier3d 0.36.1, pyo3 0.30.1 and SB3 2.9.0.

---

## 4. Milestones

Effort: **S** is a few hours, **M** is about a session, **L** is several sessions. A milestone is done only when every SPEC §13 box for it is ticked and the CLAUDE.md checks are clean.

| Milestone | Scope | Size | PRs |
|---|---|---|---|
| **M0 Scaffold** | workspace, 5 crates, Python package, Vite app, CI (3 jobs), docs, SPEC/CLAUDE updates from §3 and §7 | S | 2 |
| **M1 Drivable car** | configs, world, raycast vehicle, upright-gated wheel contact, `InputFrame`, snapshot/restore, determinism, `drive` example, golden trace generator | M | 3 |
| **M2 Browser viewer** | wasm `Game`, renderer, chase camera, fixed-step loop + interpolation, wasm golden-trace test, **perf probe** (ms per tick) | M | 2 |
| **M3 Thrusters, tank, modes, events** | nozzles + valve lag, tank, modes, Hop/Boost, events, **explosion + respawn**, Chaos; web flames, nozzle map, air gauge, toasts, Manual | L | 4 |
| **M4 PD baseline + Rust eval** | PD + allocation, Stages A/B, Rust eval runner, Off vs PD numbers, controller toggle | M | 2 |
| 🛑 **Review gate 1** | You review M0–M4 and the measured sim speed before we start spending hours on training | | |
| **M5 Python env + first policy** | `ObsSpec`, reward, domain randomization, `BatchedSim`, SB3 env, train/eval/export (plus **up to 8 checkpoints as policy JSON**, D6), parity test, **training harness (§5)**, Stage A policy | L | 4 |
| **M6 AI in the browser** | `load_policy` with hash check, AI toggle, drop-test button, wasm-vs-Python eval check, **training viewer** (a crowd of cars replays checkpoint 1, 2, 3… on Stage A drops; D6) | M | 2 |
| **M7 Courses + full curriculum** | all piece types (plain-coloured, D7), an **obstacle arena** course for the training viewer (D6), `gauntlet.json`, procedural generator, driver bot, Stages C/D/E, full training, full eval matrix, Gauntlet playable with checkpoints + results screen | L | 4 |
| 🛑 **Review gate 2** | You review the policy (by playing it) and the eval numbers before polish | | |
| **M8 Polish + write-up** | ghost, title screen, results tab, reduced motion, F3 overlay, static deploy, **hover-hack experiment**, charts, README | M | 3 |
| **M9 Optional** | anatomy figure, pressure falloff + air mass, gamepad | — | — |

The milestone contents follow SPEC §13. Tests per milestone follow SPEC §12, plus the crash, explosion and respawn tests in §3.1.

---

## 5. How we train

### 5.1 What we're training
- **Algorithm:** PPO (Stable-Baselines3) with the hyperparameters in SPEC §9.10: 64 envs, `n_steps` 512, a 128×128 Tanh MLP, and `VecNormalize`. It's an on-policy method with a small network, so it runs on CPU. **A GPU wouldn't help here**: almost all the time goes into stepping the physics, not into the network.
- **Environment:** `BatchedSim` (Rust, PyO3) steps all 64 worlds in parallel with `rayon`, so the simulation uses every CPU core while Python only sees one batched call per step.
- **Curriculum (SPEC §9.6, §9.9):** A (drops, 5 difficulty levels) → B (side tip-overs) → C (hops, with or without a car row) → D (procedural courses with the driver bot) → E (chaos knocks). A stage unlocks when the last 500 episodes pass 85% success. After that, stages are sampled by weight, so earlier skills aren't forgotten.

### 5.2 How long it should take (estimate, measured in M5)

| Item | Estimate | How we check it |
|---|---|---|
| Sim cost | ~50–100 µs per env per control step (2 physics steps + raycasts) | measured in M2 (wasm) and M5 (native) |
| End-to-end throughput on this 4-core box | **3k–10k env-steps/s** (rollout + PPO update) | first lines of the first M5 run |
| Stage A to ≥ 95% (Arcade) | ~10–30 M steps → **0.5–3 h** | curriculum log |
| Full curriculum A→E | ~100–200 M steps → **5–20 h** | curriculum log |
| Hover-hack experiment (2 runs, Arcade, Stage A only) | ~10 M steps each → **1–2 h total** | |

If the measured throughput comes in below 3k steps/s, I'll profile `BatchedSim` first (the usual culprits are Python↔Rust copies, raycasts and collider count) before changing anything else.

### 5.3 Where it runs: resumable chunks
This cloud environment is a good place to train **if** training survives the container being reclaimed. Background commands are capped at about 2 hours, and the container disappears when idle. So training is built to stop and resume at any time:

1. **`python -m ninelives.train --config … --run-id … --resume`** continues from the last checkpoint. That checkpoint includes the PPO model, the `VecNormalize` statistics, the curriculum state (stage, level, success windows) and the RNG seeds.
2. **Chunks:** each training command runs for at most **60 minutes** (`--max-minutes 60`), writes a checkpoint, and exits cleanly. I run chunks in the background, check the logs between them, and start the next one.
3. **Saving checkpoints outside the container:** the checkpoint files are small. A 128×128 policy plus the normalization stats is well under 1 MB. After each chunk I push the **latest and best** checkpoints to a separate orphan branch, `ckpt/<run-id>`. That branch never merges into `main`, so `main` stays clean and the CLAUDE.md rule "never commit `runs/`" still holds for `main`. A new container restores from that branch and resumes. *(This is a small exception to the rule, and it gets a DECISIONS entry.)*
4. **Every chunk is logged** in `docs/EXPERIMENTS.md`: date, config, seed, steps so far, wall time, success per stage and notes. That follows CLAUDE.md.
5. **After each stage unlocks:** export `models/policy.json`, run the fixed-seed eval, watch a replay for odd behaviour, and commit the policy and results on the feature branch for your PR review.

**Fallback: your machine.** The same commands work locally (`docs/TRAINING.md` will list them step by step for macOS, Linux and Windows). A machine with 8+ cores trains about 2× faster than this one. You'd push the `ckpt/<run-id>` branch, or send me the final checkpoint folder, and I'll take over export and eval.

### 5.4 Keeping training honest
- **Reward hacking:** for every run, I watch a replay in the browser (M6+) or a headless trajectory plot, not just the score. The hover hack (SPEC §9.11) is run on purpose in M8 to show what that failure looks like.
- **Fixed seeds:** `seed` is set in the config. The same config and seed reproduce the same curriculum decisions. PPO itself on CPU with torch is close to repeatable but not bit-exact, and the logs will say so.
- **Baseline:** if PPO stalls, the PD controller from M4 is the working fallback in the game. I'll try reward weights, then a bigger network, then SAC (SPEC §9.10), and log each attempt.

---

## 6. Workflow: PRs, reviews, skills

- **Branches:** one short-lived branch per small feature off `main` (e.g. `claude/m1-vehicle`), with one PR each into `main`. *Needs your OK; see §9 Q1.*
- **Authorship:** the GitHub connection acts as `Bhavya873`, so PRs show you as the author. Commits are authored as `Bhavya Modi <51858604+Bhavya873@users.noreply.github.com>` with a `Co-Authored-By: Claude` trailer. GitHub doesn't let a PR's author formally **approve** it, but you can review, comment and merge. If branch protection requires an approval, turn that off for `main`.
- **PR size:** aim for under ~400 changed lines (excluding lockfiles), one idea per PR. Each description has: what and why, how to test it, screenshots or GIFs for anything visual, and the spec section it covers.
- **Before every PR**, in this order:
  1. `/simplify`;
  2. `/code-review`;
  3. `/security-review` for anything that parses external input (course JSON, policy JSON, `localStorage`);
  4. the full checks from `CLAUDE.md`.
- **Planning skills:** `engineering:architecture` for the ADRs in M0 (determinism strategy, the Rust↔Python binding, the training harness); `engineering:testing-strategy` for the test plan; `engineering:documentation` for the README and `docs/TRAINING.md`; `engineering:deploy-checklist` before the Railway deploy.
- After opening each PR, I'll watch it and fix CI failures and review comments.

---

## 7. Coding standard: "a junior developer wrote this"

This goes into `CLAUDE.md` § Style. Readable beats clever, everywhere.

- **Names say what and in what unit:** `speed_m_s`, `air_left_kg`, `tilt_rad`. No one-letter names except loop indices and maths that copies a spec formula, which gets a comment citing the section.
- **Small files, small functions:** a function does one thing and fits on a screen (~40 lines). A file covers one concept (`tank.rs`, `valve_lag.rs`).
- **Plain constructs:** structs, enums, `for` loops and `match`. No macros we wrote ourselves, no trait-object or generic gymnastics, no iterator chains longer than about 3 steps, no lifetimes beyond the obvious. Rust traits only where the spec requires one (`Controller`).
- **Comments:** every public item gets a `///` doc comment saying what it is and its units. Inline comments say **why**, not what. Every formula comment links its spec section, e.g. `// SPEC §7.5: mass flow = force / exhaust speed`. *(This replaces CLAUDE.md's "comments only where the code cannot explain itself".)*
- **Errors:** `Result` + `thiserror` with messages a person can act on ("course.json: piece 4 'ramp' is missing 'length'").
- **Tests read like examples:** names like `firing_front_left_lift_raises_left_front_corner`, with an arrange / act / assert layout.
- **Python:** type hints, short modules, no classes where a function will do.
- **TypeScript:** strict mode, small modules, no framework, plain functions; three.js setup code kept separate from game logic.
- **Every module starts with a 2–4 line header comment** saying what it does and how it fits into the whole.

---

## 8. Deliverables checklist

| # | Deliverable | Where | Proven by |
|---|---|---|---|
| 1 | Deterministic sim: car, 10 thrusters, tank, modes, events, crash + respawn | `crates/nl-sim` | Rust tests, native and wasm golden trace |
| 2 | PD baseline + allocation | `crates/nl-control` | tests; Off vs PD eval |
| 3 | Policy runtime | `crates/nl-policy` | parity < 1e-4 |
| 4 | Python binding + SB3 env | `crates/nl-py`, `python/ninelives` | `check_env`, batched-equals-single |
| 5 | Resumable training harness + curriculum | `python/ninelives/train.py`, `configs/train/` | resume test (stop and resume gives the same curriculum state), smoke train in CI |
| 6 | Training guide | `docs/TRAINING.md` | you can run it on your own machine |
| 7 | Trained policy | `models/policy.json` | eval results |
| 8 | Evals + charts (Off/PD/AI × Stages A–E × both modes, Gauntlet completion) | `results/` | committed JSON/CSV/SVG |
| 9 | Browser game: Gauntlet, explosion + respawn, HUD, modes, controllers, ghost | `web/`, `crates/nl-wasm` | Playwright smoke, 60 fps measurement |
| 10 | Hover-hack experiment | `docs/EXPERIMENTS.md`, README | two clips + metrics |
| 11 | Static deploy | Railway (static file server container) | URL |
| 12 | README | `README.md` | your review |
| 13 | CI (rust, python, web), DECISIONS and EXPERIMENTS logs | `.github/`, `docs/` | green on every PR |

---

## 9. Owner answers (2026-10-17)

1. **Branches:** yes. One `claude/<feature>` branch per PR, plus `ckpt/<run-id>` checkpoint branches.
2. **Training location:** train here in resumable 1-hour chunks, with the owner's machine as the fallback.
3. **Review speed:** the owner reviews PRs within about a day.
4. **Deploy:** the game is hosted on **Railway** as a static site. The repo goes public once there's a working deliverable.
