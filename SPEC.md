# Nine Lives — Project Spec

> A car that always lands on its wheels. You drive through a wild obstacle course; an AI fires the car's thrusters automatically to save it whenever it is launched, knocked or flipped.

Status: v1 spec, ready to build · Owner: Bhavya Modi · Last updated: 2026-10-17

---

## 0. How to use this document

- This is the source of truth for **what** to build. `CLAUDE.md` holds the working rules for **how** to build it.
- Build milestone by milestone (section 13). Each milestone has acceptance criteria; a milestone is done only when all of them pass.
- Numbers marked *(start)* are starting values to tune, not requirements. Numbers marked *(req)* are requirements.
- When a decision here turns out wrong, change this file and add one line to `docs/DECISIONS.md` (date, decision, why).

---

## 1. Background and pitch

On 2026-10-15 Tesla revealed a Roadster with a SpaceX-developed package of 10 cold-gas thrusters fed by a compressed-air tank (reported at about 260 kg of air). In the demo the car hopped over a row of parked cars, flipped itself on a halfpipe with thruster pulses, and landed back on its wheels. The demo car was remote-operated, and the flights were short bursts, not sustained flight. Sources: [TechCrunch](https://techcrunch.com/2026/10/15/tesla-reveals-spacex-powered-roadster-that-flies-and-falls-with-style/), [Glitchwire](https://glitchwire.com/news/the-tesla-roadster-hovered-flipped-and-landed-the-air-tank-weighed-260-kilos/).

**Nine Lives** asks: can a learned controller fire 10 thrusters well enough that a human can drive recklessly and the car still always lands on its wheels, using as little air as possible?

- The human drives (steer, throttle, brake) and presses **Hop** and **Boost**.
- The controller (an AI policy, or a classical PD baseline) owns all 10 nozzles and fires them automatically.
- The project is "inspired by" the Roadster. Never use Tesla or SpaceX names, logos or car designs in the product. The car is a generic low-poly sports car.

---

## 2. Goals and non-goals

### Goals (v1)
1. A deterministic 3D physics sim of a car with 10 thrusters and an air tank, written once in Rust and used for both training and the browser game.
2. A classical baseline controller (PD attitude control + thruster allocation).
3. An RL policy (PPO) trained in that sim that auto-fires the thrusters to recover from launches, flips and knocks, then lands on all four wheels.
4. A browser game: drive a 3D obstacle course, toggle the controller between Off / PD / AI, switch Realistic / Arcade modes, see every nozzle fire.
5. A reproducible evaluation that compares Off vs PD vs AI on fixed seeds, with charts for the README.
6. A documented reward-hacking experiment ("the hover hack") and its fix.

### Non-goals (v1)
- Hardware (no microcontroller, no ESP32).
- AI that drives the car (steering/throttle stay human or scripted).
- Photorealistic graphics, multiplayer, mobile touch controls, a backend server.
- Real Roadster specs. Numbers are plausible, not official.

---

## 3. Glossary

| Term | Meaning |
|---|---|
| Nozzle / thruster | One of the 10 cold-gas thrusters. Commanded by a throttle in [0, 1] (valve duty). |
| Controller | Whatever produces the 10 throttles each control step: `Off`, `Manual`, `Pd`, `Ai`. |
| Mode | Physics parameter preset: `Realistic` or `Arcade` (section 7.6). |
| Feed-forward | Throttle added by the game for Hop and Boost, on top of the controller. |
| Airborne | No wheel ray in contact with ground. |
| Upset | An episode segment that starts when the car is airborne for > 0.3 s, or its tilt exceeds 60°. |
| Save | An upset that ends with all 4 wheels in contact, tilt < 15°, and no crash, held for 0.5 s. |
| Crash | A hard body impact, or touching the ground upside down (section 7.9). In the game the car explodes and respawns. |
| Tilt | Angle between the car's up axis and world up. |

---

## 4. Conventions *(req)*

- **Units:** SI everywhere (m, s, kg, N, rad). Angles in radians in code; degrees only in UI text.
- **World frame:** right-handed, **+Y up**, gravity (0, −9.81, 0). Matches three.js.
- **Car body frame:** origin at the centre of mass. **+X forward, +Y up, +Z right.** (Check: forward × up = +X × +Y = +Z = right.)
- **Rotation signs** (right-hand rule in the body frame):
  - roll about +X: positive means the left side rises;
  - yaw about +Y: positive means the nose turns left;
  - pitch about +Z: positive means the nose rises.
- **Floats:** `f32` in the sim, policy and wasm; numpy `float32` at the Python boundary.
- **Time:** fixed physics step `dt = 1/120 s`; control step every 2 physics steps (60 Hz).
- **Randomness:** only seeded `rand_chacha::ChaCha8Rng`. No `thread_rng`, no system time in sim code.
- **Nozzle order** (index used everywhere: actions, observations, UI, exports): `0 L_FL, 1 L_FR, 2 L_RL, 3 L_RR, 4 S_FL, 5 S_RL, 6 S_FR, 7 S_RR, 8 B_L, 9 B_R`.
- **Wheel order:** `0 FL, 1 FR, 2 RL, 3 RR`.

---

## 5. Architecture

```
                 configs/*.toml  courses/*.json
                          │
              ┌───────────▼────────────┐
              │  nl-sim (Rust, rapier) │  ← the only physics, events, obs, reward
              │  nl-control (PD + alloc)│
              │  nl-policy (MLP fwd)   │
              └─────┬────────────┬─────┘
          PyO3/maturin│            │wasm-bindgen
          ┌─────────▼──┐      ┌──▼──────────────────┐
          │ nl-py       │      │ nl-wasm             │
          │ BatchedSim  │      │ Game (tick, render) │
          └─────┬──────┘      └──┬──────────────────┘
     Python      │                 │  TypeScript
 ┌───────────────▼─────┐   ┌───────▼─────────────────────┐
 │ ninelives (Gymnasium,│   │ web/ (three.js, HUD, input) │
 │ SB3 PPO, eval, export)──►│ loads models/policy.json    │
 └──────────────────────┘   └─────────────────────────────┘
```

**Single source of truth** *(req)*: physics, thruster forces, the air tank, events, observations and rewards live only in `nl-sim`. Python and TypeScript never re-implement them. The observation layout is defined once in Rust and exported as a spec with a hash.

---

## 6. Repository layout and stack

```
ninelives/
├── CLAUDE.md
├── SPEC.md
├── Cargo.toml                 # workspace
├── rust-toolchain.toml        # stable, with wasm32-unknown-unknown target
├── crates/
│   ├── nl-sim/                # world, vehicle, thrusters, tank, modes, courses, events, obs, reward, scenarios, driver bot
│   ├── nl-control/            # Controller trait, Off, Manual, Pd, thruster allocation
│   ├── nl-policy/             # policy JSON loader + MLP forward pass + obs normalization
│   ├── nl-py/                 # PyO3 module `ninelives_sim` (BatchedSim, obs spec, eval helpers)
│   └── nl-wasm/               # wasm-bindgen `Game` API for the browser
├── python/
│   ├── pyproject.toml         # package `ninelives`
│   ├── ninelives/
│   │   ├── env.py             # single Gymnasium env (for checks) + SB3 VecEnv adapter over BatchedSim
│   │   ├── train.py           # PPO training + curriculum callback
│   │   ├── eval.py            # fixed-seed evaluation → results/*.json, *.csv
│   │   ├── export.py          # SB3 model + VecNormalize → models/policy.json
│   │   └── charts.py          # README charts from results/
│   └── tests/
├── web/
│   ├── package.json, vite.config.ts, tsconfig.json
│   ├── index.html
│   └── src/ (main.ts, render/, hud/, input/, ghost/, wasm/)
├── configs/
│   ├── vehicle.toml           # chassis, wheels, nozzles, tank
│   ├── modes.toml             # realistic, arcade
│   ├── reward.toml            # reward weights
│   ├── scenarios.toml         # stage spawn distributions, domain randomization
│   └── train/*.yaml           # PPO hyperparameters per run
├── courses/
│   ├── gauntlet.json          # the demo course
│   └── pieces.json            # piece catalogue for procedural training courses
├── models/policy.json         # exported policy (small, committed)
├── results/                   # eval outputs (committed: json/csv/svg only)
├── runs/                      # training checkpoints (git-ignored)
├── docs/DECISIONS.md, docs/EXPERIMENTS.md
└── .github/workflows/ci.yml
```

### Stack
| Area | Choice | Notes |
|---|---|---|
| Physics | `rapier3d` with features `enhanced-determinism`, `serde-serialize` | 0.36.1 was current on 2026-10-16. Uses `control::DynamicRayCastVehicleController` for wheels. |
| Rust ↔ Python | `pyo3` + `maturin` + `numpy` crate | pyo3 0.30.1 current on 2026-10-16 (needs Rust ≥ 1.85). |
| Rust ↔ browser | `wasm-bindgen` (via `wasm-pack` or `wasm-bindgen-cli`) | Build `--target web`. Run `wasm-opt -O3` on release builds. |
| Parallel envs | `rayon` over independent worlds | Rapier's own `parallel` feature stays off. |
| RL | Python 3.11 or 3.12, `gymnasium`, `stable-baselines3`, `torch` (CPU is fine) | SB3 2.9.0 current on 2026-10-16 (Python ≥ 3.10). |
| Web | TypeScript, Vite, `three` | No UI framework needed. |
| Tests | `cargo test`, `pytest`, Playwright | |
| Lint | `rustfmt`, `clippy -D warnings`, `ruff`, `eslint`, `prettier` | |

Pin the latest stable versions at scaffold time and record them in lockfiles (`Cargo.lock`, `package-lock.json`, `uv.lock` or `requirements.lock`). Read the docs for the pinned Rapier version; its vehicle and query APIs change between releases.

---

## 7. Simulation (`nl-sim`)

### 7.1 Stepping
- Physics `dt = 1/120 s` *(req)*. The controller runs every 2 physics steps (60 Hz) and its throttles are held between control steps.
- Per physics step, in this order:
  1. read driver input;
  2. update feed-forward (Hop, Boost);
  3. at control ticks, get controller throttles;
  4. combine and clamp;
  5. apply valve lag;
  6. gate by the air tank;
  7. apply nozzle forces;
  8. update the vehicle controller;
  9. step the pipeline;
  10. update events and the tank;
  11. compute observations and reward at control ticks.
- `World::snapshot()` / `World::restore()` serialize the full state (serde) for checkpoints and tests.

### 7.2 World
- Ground: large flat cuboid or heightfield, friction 1.0 *(start)*; randomized in training.
- Course pieces are static colliders (cuboids, convex hulls, trimeshes) built from course JSON (7.11).
- Air drag on the chassis: `F = −0.5 · ρ · CdA · |v| · v`, with ρ = 1.2 and CdA = 0.6 m² *(start)*, plus optional wind.

### 7.3 Vehicle
| Parameter | Value |
|---|---|
| Dry mass | 1800 kg *(start)* |
| Chassis collider | cuboid, half-extents (2.1, 0.45, 1.0) m, plus a thin roof cuboid on top so the roof contact can be detected separately |
| Inertia | set explicitly ≈ (850, 3250, 2900) kg·m² for (roll X, yaw Y, pitch Z) *(start)*; a real car is not a uniform box |
| Wheels | `DynamicRayCastVehicleController`, 4 suspension mounts at (±1.3, 0.0, ±0.85), direction −Y, axle ±Z, radius 0.34 m, rest length 0.3 m, max travel 0.2 m *(start)*. This gives a centre of mass ≈ 0.6 m above the ground and ≈ 0.15 m ground clearance at rest |
| Suspension | stiffness, compression and relaxation damping, max force: tune for stable landings from 2 m drops |
| Drive | AWD; engine force up to 6 kN per wheel *(start)*; speed cap 45 m/s |
| Steering | max 0.5 rad at low speed, scaled down to 0.15 rad at 40 m/s; rate-limited |
| Brakes | brake impulse per wheel *(start: tune)*; handbrake not needed |
| Wheel contact | from each wheel's `raycast_info()`. Count a wheel as touching only if its ray hits **and** the car is upright enough (up·world_up > 0) so upward rays on an inverted car never count as contact. |

The air mass adds to the dry mass (update mass properties when the tank changes by > 5 kg) *(start: optional in v1, required in v2)*.

### 7.4 Thrusters *(req layout; start forces)*
Force direction is the force **on the car** (opposite to the exhaust). Positions are in the body frame relative to the centre of mass.

| # | Id | Position (m) | Force dir | Max force (Realistic) | Main role |
|---|---|---|---|---|---|
| 0 | L_FL | (+1.6, −0.3, −0.85) | +Y | 12 kN | lift, roll, pitch |
| 1 | L_FR | (+1.6, −0.3, +0.85) | +Y | 12 kN | lift, roll, pitch |
| 2 | L_RL | (−1.6, −0.3, −0.85) | +Y | 12 kN | lift, roll, pitch |
| 3 | L_RR | (−1.6, −0.3, +0.85) | +Y | 12 kN | lift, roll, pitch |
| 4 | S_FL | (+1.4, +0.5, −1.0) | +Z | 4 kN | roll, yaw, lateral |
| 5 | S_RL | (−1.4, +0.5, −1.0) | +Z | 4 kN | roll, yaw, lateral |
| 6 | S_FR | (+1.4, +0.5, +1.0) | −Z | 4 kN | roll, yaw, lateral |
| 7 | S_RR | (−1.4, +0.5, +1.0) | −Z | 4 kN | roll, yaw, lateral |
| 8 | B_L | (−2.1, 0.0, −0.4) | +X | 8 kN | boost, yaw |
| 9 | B_R | (−2.1, 0.0, +0.4) | +X | 8 kN | boost, yaw |

Torques at full force, `τ = r × F`, in kN·m as (roll X, yaw Y, pitch Z). These are the sign checks for the unit tests:

| Nozzle | Torque |
|---|---|
| L_FL | (+10.2, 0, +19.2) |
| L_FR | (−10.2, 0, +19.2) |
| L_RL | (+10.2, 0, −19.2) |
| L_RR | (−10.2, 0, −19.2) |
| S_FL | (+2.0, −5.6, 0) |
| S_RL | (+2.0, +5.6, 0) |
| S_FR | (−2.0, +5.6, 0) |
| S_RR | (−2.0, −5.6, 0) |
| B_L | (0, −3.2, 0) |
| B_R | (0, +3.2, 0) |

The 6×10 allocation matrix (force + torque per unit throttle) has rank 6, so every axis is controllable. Thrust is one-sided: there is no downward or backward thrust.

- **Valve lag:** first-order, `u_eff += (u_cmd − u_eff) · (1 − exp(−dt/τ))`, with τ = 0.03 s *(start)*.
- **Applying forces:** each physics step, reset the chassis forces once, then add drag and call `add_force_at_point` per nozzle with the world-space force and point (rotate both by the chassis pose).

### 7.5 Air tank
- Mass flow per nozzle: `ṁ = F_actual / v_e`, with `v_e = 600 m/s` (cold-gas air). At full lift (48 kN) that is 80 kg/s.
- **Realistic:** capacity 260 kg (the reported figure), so full lift lasts about 3.25 s. Refill +5 kg/s while ≥ 3 wheels touch. This refill rate is a gameplay concession; label it so in the UI help text.
- **Empty tank:** thrust scales to 0 smoothly over the last 2% of air, so nothing pops.
- **Pressure falloff** *(v2, flag)*: `F_max · (0.5 + 0.5 · fill)`.
- **Arcade:** unlimited air. Still track "air used" for stats.

### 7.6 Modes
| Parameter | Realistic | Arcade |
|---|---|---|
| Thrust scale (× table 7.4) | 1.0 | 2.0 |
| Tank | 260 kg, refill 5 kg/s | unlimited |
| Hop feed-forward | 0.75 on L_* for 0.40 s | 0.75 on L_* for 0.35 s |
| Hop apex, flat ground, controller Off (computed) | ≈ 1.7 m | ≈ 7.5 m |
| Hop air cost | ≈ 24 kg | n/a |
| Boost feed-forward | 1.0 on B_* while held | 1.0 on B_* while held |
| Boost extra acceleration | ≈ 0.9 g | ≈ 1.8 g |
| Hop cooldown | 1.5 s | 0.8 s |

Mode parameters are a struct `ModeParams { thrust_scale, unlimited_air, tank_capacity_kg, refill_kg_s, hop_ff, hop_s, hop_cooldown_s }`. Training samples it continuously (9.8), so a single policy serves both modes.

### 7.7 Throttle pipeline *(req)*
```
u_ctrl ∈ [0,1]^10      from the controller (held for 2 physics steps)
u_ff   ∈ [0,1]^10      Hop: hop_ff on 0..3 while hop active; Boost: 1.0 on 8..9 while held
u_cmd  = clamp(u_ff + u_ctrl, 0, 1)
u_eff  = valve_lag(u_cmd)
F_i    = u_eff_i · F_max_i · thrust_scale · tank_gate
```
During a Hop the controller cannot reduce lift below `hop_ff`, but it can add up to the remaining 0.25 on any lift nozzle for differential attitude control, and it has full use of the other 6 nozzles.

### 7.8 Hop and Boost
- **Hop** is allowed when ≥ 3 wheels touch, the cooldown has expired, and (Realistic) there is ≥ 30 kg of air. Otherwise the HUD shows why ("No air", "Cooldown").
- **Boost** only fires while the button is held; it is allowed in the air too.

### 7.9 Events *(req definitions; start thresholds)*
- **Airborne start/end:** wheel-contact count goes to 0 (no wheel touches) / back above 0 (any wheel touches).
- **Upset start:** airborne for > 0.3 s, or tilt > 60°.
- **Save:** during an upset, 4 wheels touching, tilt < 15°, |ω| < 1.5 rad/s, held for 0.5 s. Emits `Save { duration, air_used, peak_tilt }`.
- **Crash:** any chassis or roof collider contact with relative normal speed > 6 m/s (`HardImpact`); **or** any roof contact while tilt > 120°, at any speed (`UpsideDown`); **or** stuck (tilt > 100° for > 2 s) (`Stuck`); **or** out of bounds (y < −20 or outside the course box) (`OutOfBounds`). Emits `Crash { cause }`.
- **Explosion and respawn (game only):** on `Crash` the sim freezes the wreck (no physics steps) and the game plays an explosion (flash, shockwave ring, about 20 low-poly debris pieces, camera shake unless reduced motion is on). After `respawn_delay_s` (1.5 s) of sim time (auto-respawn, which only the game turns on) the sim puts the car back at its last checkpoint (pose and velocities, not a `restore`) with a full tank and emits `Respawn`; mode, clock and random generator carry on. The HUD's "Lives used" counter is the crash count, which the game keeps as it drains events. The Reset button (R) respawns at once. In training a crash simply ends the episode (9.5).
- Events are drained by the caller (`world.drain_events()`) and counted in stats.

### 7.10 Determinism *(req)*
- Enable `rapier3d/enhanced-determinism`. Same seed, config and input sequence must produce bit-identical state on native and wasm.
- No iteration over `HashMap` in sim logic; use `Vec` or `BTreeMap`.
- A golden trace (2000 steps, scripted inputs) is generated natively and checked in CI on both native and wasm (Node test).

### 7.11 Courses
Course JSON, schema version 1:
```json
{
  "version": 1,
  "name": "Gauntlet",
  "start": { "pos": [0, 0.6, 0], "yaw": 0 },
  "bounds": { "min": [-50, -20, -50], "max": [400, 60, 50] },
  "pieces": [
    { "type": "ramp", "pos": [40, 0, 0], "yaw": 0, "length": 12, "width": 6, "angle_deg": 18 },
    { "type": "checkpoint", "pos": [60, 0, 0] },
    { "type": "kicker", "pos": [80, 0, 0], "yaw": 0, "length": 6, "width": 3, "angle_deg": 20, "side": "left" },
    { "type": "gap", "pos": [120, 0, 0], "length": 14 },
    { "type": "car_row", "pos": [170, 0, 0], "count": 5, "height": 1.4, "hop_hint": true },
    { "type": "halfpipe", "pos": [220, 0, 0], "radius": 6, "length": 20 },
    { "type": "bumps", "pos": [270, 0, 0], "count": 8, "height": 0.4 },
    { "type": "kick_plate", "pos": [300, 0, 0], "impulse": [0, 4000, 3000], "angular": [2500, 0, 0] },
    { "type": "wall", "pos": [340, 0, 0], "height": 3, "with_ramp": true },
    { "type": "finish", "pos": [380, 0, 0] }
  ]
}
```
- The course runs along world **+X**. A piece's local +X is its run direction and `yaw` rotates it about +Y, the same convention as the car, so `yaw: 0` everywhere means a straight course.
- `car_row` boxes are 4.5 m long, 1.9 m wide and `height` tall, with 0.6 m gaps between them.
- Piece types: `ramp`, `kicker` (a one-sided ramp that rolls the car), `gap`, `car_row` (static boxes; `hop_hint` tells the driver bot to Hop), `halfpipe`, `bumps`, `kick_plate` (applies impulses in N·s and N·m·s on contact, once per pass), `wall`, `landing_pad`, `checkpoint`, `finish`.
- **Gauntlet** (the demo course), in order: start straight → kicker → gap → car row (Hop) → halfpipe (flip back) → bumps + kick plates → big ramp to a landing pad → wall with ramp → finish. A checkpoint after each section.
- **Procedural generator** (training): 4–8 pieces sampled from `pieces.json` with random parameters inside safe ranges, joined by straights of 20–40 m.

### 7.12 Disturbances
- `kick_plate` in courses.
- **Chaos** (game button and training stage E): one linear impulse ≤ 8 kN·s (≈ 4.4 m/s) plus an angular impulse ≤ 3 kN·m·s in random directions.

---

## 8. Controllers (`nl-control`, `nl-policy`)

```rust
pub trait Controller {
    fn reset(&mut self, seed: u64);
    fn act(&mut self, obs: &Obs, ctx: &ControlCtx) -> [f32; 10]; // throttles in [0,1]
}
```

1. **Off:** all zeros (feed-forward still works).
2. **Manual** (debug only): keys map to individual nozzles at throttle 1.
3. **Pd** (baseline):
   - **Attitude:** the desired up direction in the body frame is `up_des = −g_dir_body`; the error axis is `e = (0,1,0) × up_des`; the desired torque is `τ = I · (kp·e − kd·ω)`, with kp = 8 and kd = 4 *(start)*.
   - **Vertical, airborne and roughly upright (up·world_up > 0.7):** if height < 3 m and downward speed > 4 m/s, command `F_y = m · (g + kv · (v_down − 1.5))`, with kv = 2. Otherwise `F_y = 0` (saves air).
   - **Allocation:** solve `min ‖W(B·u − w)‖² + λ‖u‖²` subject to `0 ≤ u ≤ 1`, with `w = [0, F_y, 0, τx, τy, τz]` and `B` the 6×10 matrix scaled by max force. Use projected gradient descent (30 iterations, step `1/L`) or a small active-set NNLS. Torque weights are higher than force weights.
   - **On the ground and inverted:** use roll torque only, to flip back.
4. **Ai:** loads `models/policy.json`, normalizes the observation, runs the MLP forward pass, and maps the action from [−1, 1] to [0, 1]. The policy is deterministic (mean action).

The PD controller is required before RL. It proves the problem is controllable and is the baseline the AI must be compared against.

---

## 9. RL environment and training

### 9.1 Python binding
`ninelives_sim.BatchedSim(n_envs, config_json, seed)`:
- `obs_spec() -> list[dict]` (name, index, low, high) and `obs_schema_hash() -> str`.
- `reset(seeds: np.ndarray[uint64] | None) -> np.ndarray[f32, (N, OBS_DIM)]`.
- `step(actions: np.ndarray[f32, (N, 10)]) -> (obs, reward[f32,N], terminated[bool,N], truncated[bool,N], info: dict of arrays)`. Actions are in [−1, 1].
- **Auto-reset:** an env that ends returns its terminal observation in `info["terminal_obs"]` (N × OBS_DIM, masked) and its reset observation in `obs`.
- `info` arrays: `saves`, `crashes`, `crash_cause`, `air_used_kg`, `stage`, `success`, `episode_len`.
- Envs are stepped in parallel with `rayon`; each env owns its own world and RNG.

`ninelives.env`:
- `NineLivesVecEnv(sb3.common.vec_env.VecEnv)` adapts `BatchedSim` to SB3. It sets `infos[i]["terminal_observation"]` and `infos[i]["TimeLimit.truncated"]`.
- `NineLivesEnv(gymnasium.Env)` is a single-env wrapper, used only for `gymnasium.utils.env_checker.check_env` and debugging.

### 9.2 Observation (OBS_DIM = 41, v1) *(req layout)*
| Index | Name | Notes |
|---|---|---|
| 0–2 | gravity_dir_body | unit vector of world-down in the body frame |
| 3–5 | ang_vel_body | rad/s |
| 6–8 | lin_vel_body | m/s |
| 9 | height | world-down ray from the centre of mass to the ground, capped at 20 m |
| 10–13 | wheel_contact | FL, FR, RL, RR, each 0 or 1 |
| 14–17 | suspension_compression | 0..1 per wheel |
| 18 | air_fraction | 1.0 when air is unlimited |
| 19 | thrust_scale | relative to the Realistic baseline |
| 20 | unlimited_air | 0 or 1 |
| 21 | hop_active | 0 or 1 |
| 22 | hop_time_left | s |
| 23 | boost_active | 0 or 1 |
| 24 | driver_throttle | −1..1 (brake is negative) |
| 25 | driver_steer | −1..1 |
| 26–30 | terrain_probe | ground height relative to the centre of mass at 2, 4, 6, 8 and 10 m ahead along the horizontal heading (downward world rays), capped |
| 31–40 | prev_action | the last 10 throttles |

The layout lives in Rust (`ObsSpec`). The schema hash covers names and order. Python, the export and the browser all check the hash *(req)*.

### 9.3 Action
`Box(low=−1, high=1, shape=(10,))`, mapped in the sim to `u = (a + 1) / 2`.

### 9.4 Reward (per control step; weights in `configs/reward.toml`) *(start)*
| Term | Formula | Weight |
|---|---|---|
| Tilt | `(up·world_up − 1)` | 0.05 |
| Spin | `−min(‖ω‖², 50)` | 0.002 |
| Air (limited) | `−air_used_step / tank_capacity` | 2.0 |
| Air (unlimited) | `−Σ u` | 0.002 |
| Airborne time | `−1` per step while airborne for longer than 0.5 s | 0.02 |
| Smoothness | `−‖u_t − u_{t−1}‖²` | 0.002 |
| Save (event) | `2 + 3·q`, `q = clamp(1 − ‖ω‖/4) · clamp(1 − v_down/6)` | 1 |
| Crash (event, terminal) | `−10` | 1 |

There is no survival bonus *(req)*. Being alive must not pay, or the policy learns to hover (9.11).

### 9.5 Episode end
- **Terminated:** crash; or success (Stage A/B: a save held for 1.0 s; Stage C: a save after the hop).
- **Truncated:** time limit (A/B 8 s, C 15 s, D/E 45 s).

### 9.6 Scenario stages
| Stage | Spawn | Driver | Success |
|---|---|---|---|
| A — Drop | height U(1.5, 8) m; tilt U(0, θmax) about a random axis; ω magnitude U(0, ωmax) on a random axis; horizontal v U(0, 15); vertical v U(−5, 3) | none | save |
| B — Side tip-over | resting on its side at low speed, roll U(60°, 110°) to either side, small pitch and yaw | none | back on 4 wheels, upright, without touching the roof |
| C — Hop | driving straight at U(5, 25) m/s on flat ground; Hop at a random time; in 50% of episodes a car row of 1–3 cars (1.2–1.5 m tall) placed so the hop must clear it | bot | save after the hop, no obstacle hit |
| D — Course | procedural course (7.11) | bot | reach the finish with no crash |
| E — Chaos | any of A, C, D plus 1–3 Chaos impulses at random times | bot or none | as for the base stage |

Curriculum for A: θmax goes 30° → 180° and ωmax 0 → 6 rad/s, in 5 levels.

### 9.7 Driver bot (training only)
- Pure-pursuit steering on the course centerline: lookahead `6 + 0.3·v` m; P control on speed toward a target U(10, 30) m/s.
- Steering noise: an OU process (σ = 0.1). There is a 5% chance per second of a random steering jerk of ±0.5 for 0.3 s.
- Presses Hop at `hop_hint` pieces with ±0.15 s timing noise. Never uses Manual thrusters.

### 9.8 Domain randomization (per episode) *(start)*
| Parameter | Range |
|---|---|
| Mass | × U(0.9, 1.1) |
| Centre-of-mass offset | ±0.1 m per axis |
| Inertia | × U(0.9, 1.1) |
| Max force | × U(0.85, 1.15), per nozzle |
| Valve lag τ | U(0.02, 0.06) s |
| Actuation latency | 0–2 control steps |
| Observation noise (Gaussian σ) | gravity_dir 0.01, ω 0.05 rad/s, v 0.1 m/s, height 0.05 m |
| Ground friction | U(0.6, 1.2) |
| Wind | U(−3, 3) m/s per horizontal axis |
| Thrust scale | U(1.0, 2.0) |
| Unlimited air | probability 0.4; otherwise tank U(150, 350) kg |

### 9.9 Curriculum callback
- Track the success rate over the last 500 episodes per stage. When it exceeds 85%, advance that stage's level or unlock the next stage.
- Once unlocked, sample stages with weights A 0.25, B 0.15, C 0.2, D 0.3, E 0.1, so earlier skills are not forgotten.
- Log the stage, level and success rates to TensorBoard.

### 9.10 PPO starting hyperparameters (`configs/train/base.yaml`) *(start)*
- `n_envs` 64, `n_steps` 512 (32,768 samples per update), `batch_size` 4096, `n_epochs` 10.
- `learning_rate` 3e-4 with linear decay, `gamma` 0.99, `gae_lambda` 0.95, `clip_range` 0.2, `ent_coef` 0.0.
- `policy_kwargs`: `net_arch = dict(pi=[128,128], vf=[128,128])`, Tanh activation.
- `VecNormalize(norm_obs=True, norm_reward=True, clip_obs=10)`.
- Checkpoint every 1M steps and keep the best by eval success.
- Log every run in `docs/EXPERIMENTS.md`: date, config, seed, steps, wall time, success per stage, notes.
- If PPO stalls after reasonable tuning, try SAC (same env, action space unchanged) and record the comparison.

### 9.11 Required experiment: the hover hack
1. **Broken reward:** add `+0.1` per step for being upright (a survival bonus) and remove the airborne-time cost. Train in Arcade (unlimited air). The expected outcome is a policy that hovers instead of landing. Record a clip and the mean airborne time.
2. **Fixed reward:** the default reward from 9.4. Record a clip and the same metrics.
3. Report both in the README with the explanation: pay for landing, charge for airtime, never pay for merely surviving.

### 9.12 Export (`models/policy.json`) *(req)*
```json
{
  "format": "ninelives-policy", "version": 1,
  "obs_schema_hash": "…", "obs_dim": 41, "act_dim": 10,
  "obs_norm": { "mean": [], "var": [], "eps": 1e-8, "clip": 10.0 },
  "layers": [ { "w": [], "b": [], "in": 41, "out": 128, "act": "tanh" },
              { "w": [], "b": [], "in": 128, "out": 128, "act": "tanh" },
              { "w": [], "b": [], "in": 128, "out": 10, "act": "none" } ],
  "action": { "clip": [-1, 1], "map": "(a+1)/2" },
  "trained": { "run_id": "…", "steps": 0, "git_sha": "…" }
}
```
- Weights are row-major `out × in`, `float32` values as JSON numbers. The layers come from SB3's `mlp_extractor.policy_net` plus `action_net`.
- **Parity test** *(req)*: on 1000 random observations, the Rust forward pass matches SB3 `predict(deterministic=True)` (after VecNormalize) with max absolute difference < 1e-4.

---

## 10. Evaluation

- `python -m ninelives.eval --controller {off,pd,ai} --stage {A,B,C,D,E} --mode {realistic,arcade} --episodes 1000 --seed 0`.
- A fixed seed list makes runs reproducible. Domain randomization is off by default in eval (`--dr` turns it on).
- **Outputs:** `results/<date>_<controller>_<stage>_<mode>.json` and `.csv`, with per-episode rows (success, saves, crashes, cause, air used, peak tilt, touchdown |ω|, touchdown v_down).
- **Metrics:** save rate, crash rate, air used per save (kg), touchdown spin and vertical speed (median and p95), and Gauntlet completion rate with the driver bot.
- **Charts** (`python -m ninelives.charts`, SVG into `results/`): save rate by stage for Off/PD/AI; air per save, PD vs AI; touchdown spin distributions; the training curve.

**Targets** *(start; revise after first results; report honestly either way)*:
- Stage A Arcade, AI save rate ≥ 95%; Stage A Realistic ≥ 80%; Stage B Arcade ≥ 90%.
- AI beats PD on save rate or on air per save in Realistic mode.
- Gauntlet crash rate with AI ≤ ⅓ of the crash rate with Off.

---

## 11. Web game (`web/` + `nl-wasm`)

### 11.1 Wasm API (`nl-wasm`)
```ts
class Game {
  constructor(vehicleToml: string, modesToml: string, seed: bigint);  // courseJson joins in M6
  set_mode(mode: "realistic" | "arcade"): void;
  set_controller(kind: "off" | "pd" | "ai" | "manual"): void;
  load_policy(json: string): void;          // throws if obs_schema_hash mismatches
  tick(input: Uint8Array /* packed InputFrame */): void;  // advances exactly one physics step
  chassis_pose(): Float32Array;             // [px,py,pz,qx,qy,qz,qw]
  wheel_poses(): Float32Array;              // 4 × 7
  nozzle_throttles(): Float32Array;         // 10 effective throttles
  hud(): Float32Array;                      // fixed layout: speed, air_frac, tilt, |ω|, saves, crashes, time, hop_cd, …
  drain_events(): Float32Array;             // 4 per event: EventKind, then up to 3 values (Save stats, CrashCode)
  is_crashed(): boolean;
  set_checkpoint(): void;                   // the car's pose and motion now become the respawn point
  snapshot(): Uint8Array; restore(s: Uint8Array): void;
  build_hash(): string;
}
```
`InputFrame`: throttle (i8), steer (i8), bit flags (hop, boost, chaos, reset, manual nozzle bits). It is identical to what the ghost recorder stores.

### 11.2 Main loop
- `requestAnimationFrame` with a fixed-step accumulator: `tick()` at 120 Hz, at most 8 ticks per frame (drop time beyond that). The renderer interpolates between the last two poses.
- The policy runs inside wasm (no JS inference).

### 11.3 Controls
| Key | Action |
|---|---|
| W / ↑ | throttle |
| S / ↓ | brake / reverse |
| A D / ← → | steer |
| Space | Hop |
| Shift | Boost |
| R | reset to the last checkpoint (the `reset` bit in the InputFrame) |
| C | Chaos (random knock) |
| M | toggle Realistic / Arcade |
| T | cycle controller Off → PD → AI |
| G | toggle ghost |
| 1–0 | fire nozzles 0–9 (only when the controller is Manual) |
| Esc | pause / menu |

### 11.4 Visuals
- Low-poly generic sports car (no brand shapes), flat-shaded course, a clear sky gradient, soft shadows from one directional light.
- **Nozzles:** small cones on the body at the 7.4 positions. Each flame is an additive-blended cone mesh whose length and opacity follow its effective throttle, plus a light puff of particles. Colour is a cold blue-white (cold gas), never fire.
- **Camera:** smoothed chase camera that stays world-up when the car flips, so the player sees the flip. A slight pull-back while airborne.
- Respect `prefers-reduced-motion`: no camera shake, fewer particles.

### 11.5 HUD
- **Top-left:** mode, controller, course section.
- **Bottom-left:** speed, and the air gauge (∞ in Arcade) with a red flash below 15%.
- **Bottom-right:** an attitude ball (roll/pitch), spin rate, and a top-down **nozzle map** of the car with 10 dots glowing by throttle.
- **Top-right:** saves, crashes, time, Hop cooldown.
- **Centre toasts:** "SAVE" (with air used), "CRASH" (with cause), "NO AIR".

### 11.6 Screens and features
- **Title screen:** mode and controller pickers, "How it works" (three short paragraphs plus the Section 1 sources), Play.
- **Results screen** at the finish: time, saves, crashes, air used, and the delta vs the ghost.
- **Ghost:** record `InputFrame`s per tick plus seed, course, mode, controller, policy hash and `build_hash`. Replay in a second `Game` instance and render it translucent. Store the best run in `localStorage` (wrap every read and write in try/catch; discard on build-hash mismatch).
- **Results tab:** shows the committed SVG charts from `results/`.

### 11.7 Performance and deploy
- Target 60 fps on a mid-range laptop. Budget: sim tick ≤ 1.5 ms; render ≤ 8 ms. Measure and show an optional FPS / ms overlay (F3).
- Release wasm built with `opt-level = "s"` or `"z"`, LTO and `wasm-opt`. Target wasm ≤ 2.5 MB gzipped.
- Static deploy on Railway: a small static file server container serves `web/dist` plus `models/policy.json` plus `courses/` plus `results/*.svg` (with the `application/wasm` MIME type). No game server or API.

---

## 12. Testing and CI

### Rust (`cargo test --workspace`)
- **Frame and sign tests:** each nozzle's torque sign matches the table in 7.4; firing L_FL alone raises the left front (positive roll and pitch rate after 0.1 s).
- **Allocation:** `B` has rank 6; the PD allocation returns `u ∈ [0, 1]`.
- **Tank:** 1 s of full lift in Realistic consumes 80 kg ± 1%; the empty tank fades thrust smoothly; refill only with ≥ 3 wheels down.
- **Valve lag:** step response reaches 63% at τ ± 5%.
- **Hop:** Realistic, flat ground, controller Off, centre-of-mass offset 0 → apex 1.7 m ± 20%; Arcade apex 7.5 m ± 20%.
- **Events:** an upright 2 m drop gives a landing with no crash; an upside-down 0.5 m drop gives an `UpsideDown` crash; a hard side impact at 8 m/s gives a `HardImpact` crash; 3 s at tilt > 100° without roof contact gives a `Stuck` crash; with auto-respawn on, a crashed car stays frozen for `respawn_delay_s` and then is back at its checkpoint pose with a full tank and a `Respawn` event; the Reset bit respawns at once, only once while held; without auto-respawn (training) a crash stays until reset.
- **Determinism:** two runs with the same seed and inputs are bit-identical for 2000 steps; the golden trace matches.
- **Observations:** length 41; names unique; hash stable; values inside the spec bounds on random states.
- **Policy:** the JSON loader rejects a wrong hash or shape; the forward pass matches fixtures.

### Python (`pytest python/tests`)
- `check_env(NineLivesEnv())` passes.
- `BatchedSim` with N = 4 matches 4 single envs stepped with the same seeds and actions.
- Auto-reset returns the terminal obs correctly.
- A 20k-step smoke train runs and writes a checkpoint.
- Export plus the parity test (< 1e-4).

### Web
- `tsc --noEmit`, `eslint`, `vite build`.
- Wasm golden-trace test in Node.
- **Playwright smoke test:** the page loads; after 3 s of simulated time the car has moved; switching to AI loads the policy; no console errors.

### CI (`.github/workflows/ci.yml`)
- Jobs: `rust` (fmt, clippy `-D warnings`, test), `python` (ruff, maturin develop, pytest), `web` (wasm build, golden trace, lint, build, Playwright).
- Training never runs in CI.

---

## 13. Milestones and acceptance criteria

**M0 — Scaffold**
- [x] Workspace, crates, Python package and Vite app exist and build; CI runs all three jobs green with placeholder tests.
- [x] `docs/DECISIONS.md` and `docs/EXPERIMENTS.md` exist.

**M1 — Drivable car (native)**
- [x] `nl-sim` builds a world from `vehicle.toml` and a flat course; the raycast vehicle drives, steers and brakes.
- [x] Headless example `cargo run -p nl-sim --example drive` prints a 10 s trajectory.
- [x] Determinism and snapshot/restore tests pass.

**M2 — Browser viewer**
- [ ] `nl-wasm` exposes `Game`; the web app renders the car and ground and drives with the keyboard at 60 fps.
- [x] Chase camera, the fixed-step loop and interpolation work. Golden trace matches on wasm.

**M3 — Thrusters, tank, modes, events**
- [ ] 10 nozzles per 7.4 with valve lag; the tank per 7.5; modes per 7.6; Hop/Boost per 7.8; events per 7.9.
- [ ] All sign, tank, hop and event tests from section 12 pass.
- [ ] Web: flames, nozzle map, air gauge, toasts, Manual controller with keys 1–0, Chaos button.

**M4 — PD baseline and eval harness**
- [ ] `Pd` controller with allocation; a controller toggle in the web app.
- [ ] Rust-side eval runner for Off/PD on Stages A and B (no Python needed), writing results JSON.
- [ ] PD saves visibly more than Off on Stage A (numbers recorded in `docs/EXPERIMENTS.md`).

**M5 — Python env and first policy**
- [ ] `nl-py` `BatchedSim` and `NineLivesVecEnv`; all Python tests pass.
- [ ] PPO trains on Stage A with the curriculum; the export and parity tests pass; `models/policy.json` is committed.
- [ ] `python -m ninelives.eval` produces results for Off/PD/AI on Stage A.

**M6 — AI in the browser**
- [ ] `load_policy`, with the hash check, and controller AI work in the game; drop tests in the browser match Python eval qualitatively.

**M7 — Hop, course, driver bot, full curriculum**
- [ ] Course loader with all piece types and the `gauntlet.json`; the procedural generator; the driver bot.
- [ ] Stages B–E trained; the policy is re-exported; eval on all stages and both modes.
- [ ] The Gauntlet is playable start to finish with checkpoints and the results screen.

**M8 — Polish and write-up**
- [ ] Ghost, title screen, results tab, reduced motion, the performance overlay, static deploy.
- [ ] The hover-hack experiment (9.11) with both clips.
- [ ] README: pitch, GIF, how it works, results charts, the hover hack, limits, how to run, credits and sources.

**M9 — Optional**
- [ ] Anatomy figure (car teardown: nozzles, tank, valves) for the landing page.
- [ ] Pressure falloff and the mass update in v2 physics.
- [ ] Gamepad support.

---

## 14. Risks and mitigations
| Risk | Mitigation |
|---|---|
| Training stops when the cloud container is reclaimed | Resumable 60-minute training chunks; checkpoints pushed to `ckpt/<run-id>` branches (ADR-003) |
| Native and wasm physics differ | `enhanced-determinism`, the golden-trace test on both targets |
| RL does not converge | PD baseline first; curriculum; reward weights; larger network; try SAC; log every run |
| Hover or other reward hacking | No survival bonus, an airborne cost, watch clips not only scores (9.11) |
| Raycast wheels misbehave when inverted | Contact counts only if upright enough (7.3); a separate roof collider |
| Realistic air is too scarce to recover | Tune thrust and tank within plausible ranges; the PD/AI air-per-save metric shows the trade-off |
| Browser too slow with `enhanced-determinism` (no SIMD) | Measure early in M2; reduce collider complexity; fall back to 90 Hz physics only if needed (update spec + golden trace) |
| Python/Rust packaging friction | Pin Python 3.11 or 3.12; `maturin develop` in a venv; document exact commands in the README |
| Trademark | Generic car, no Tesla/SpaceX names or logos; "inspired by" wording only |

---

## 15. Future ideas (not v1)
- AI autopilot that also drives.
- Pressure-dependent thrust and a heat model.
- Downloadable ghost runs and a shared leaderboard.
- Comparing RL with model-predictive control.

## 16. References
- Tesla Roadster thruster reveal, 2026-10-15: [TechCrunch](https://techcrunch.com/2026/10/15/tesla-reveals-spacex-powered-roadster-that-flies-and-falls-with-style/), [Glitchwire](https://glitchwire.com/news/the-tesla-roadster-hovered-flipped-and-landed-the-air-tank-weighed-260-kilos/)
- Rapier `DynamicRayCastVehicleController`: https://docs.rs/rapier3d/latest/rapier3d/control/struct.DynamicRayCastVehicleController.html
- PyO3: https://docs.rs/pyo3/latest/pyo3/
- Stable-Baselines3: https://pypi.org/project/stable-baselines3/
- Reward shaping walkthrough this project's style follows: Hari, "Training an AI to land a rocket with reward shaping"
