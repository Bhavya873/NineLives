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
4. A browser game: drive a 3D obstacle course, toggle the controller between Off / PD / AI, switch Limited air / Unlimited air modes (the `realistic` / `arcade` presets), see every nozzle fire.
5. A reproducible evaluation that compares Off vs PD vs AI on fixed seeds, with charts for the README.
6. A documented reward-hacking experiment ("the hover hack") and its fix.

### Non-goals (v1)
- Hardware (no microcontroller, no ESP32).
- AI that drives the car (steering/throttle stay human or scripted).
- Photorealistic graphics, multiplayer, a backend server. (Phone touch controls and gamepads are in scope since 2026-11-10: §11.9.)
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
│   └── src/ (main.ts, render/, hud/, input/, wasm/)
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
- Air drag on the chassis, per body axis: `F_i = −0.5 · ρ · CdA_i · |v| · v_i`, with v the velocity through the air (ground velocity minus the wind) in the body frame, ρ = 1.2 and CdA = 0.6 front (X), 8.0 top (Y), 5.5 side (Z) m² *(start)* (`[air_drag] drag_area_m2`). It acts at a centre of pressure 0.3 m behind and 0.1 m above the centre of mass *(start)* (`center_of_pressure_offset_m`), so a side wind pushes the car downwind, turns its nose into the wind and leans it over.
- Wind (`nl-sim/src/wind.rs`): a steady mean wind blowing in a horizontal direction, plus gusts: an Ornstein–Uhlenbeck process on each horizontal axis (exact step `g ← a·g + σ·√(1 − a²)·N(0,1)`, a = e^(−dt/τ)), stepped once per physics step from its own ChaCha8 stream (9) of the world seed. Levels in `[wind]` of `configs/scenarios.toml` *(start)*: Off (0 m/s), Breeze (5 m/s mean, σ 1.5 m/s, τ 2 s), Storm (15 m/s, σ 5 m/s, τ 1.5 s). The game picks a level (`Game.set_wind_level("off"|"breeze"|"storm")`, direction drawn from the seed; the HUD reads it from `HudField.WindXMS` and `WindZMS`); `World::set_wind(level, direction_yaw_rad)` sets it exactly. Training draws it per episode (§9.8). Wind is saved in snapshots.
- **Landing forecast** (`nl-sim/src/forecast.rs`, `World::forecast_landing()`): read-only. It flies a gravity-only path from the car's state (drag, wind and thrust ignored) and calls the first step within 0.6 m of the ground below the touchdown. It returns the touchdown time and point, the surface normal and tilt, the usable landing length (flat, obstacle-free surface along the heading, up to 30 m ahead and 8 m behind) and the uncertainty, the worst time and horizontal-position spread over ±2% speed and ±1° pitch samples. The stunt planner (11.8) is its only user; it changes nothing in `step()`.

### 7.3 Vehicle
| Parameter | Value |
|---|---|
| Dry mass | 1800 kg *(start)* |
| Chassis collider | cuboid, half-extents (2.1, 0.45, 1.0) m, with its bottom front and rear edges chamfered (0.6 m long, 0.3 m high: about 27° approach and departure angles, `chassis.underside_chamfer_m`) so the nose rides up halfpipe walls instead of digging in; plus a thin roof cuboid on top so the roof contact can be detected separately |
| Inertia | set explicitly ≈ (850, 3250, 2900) kg·m² for (roll X, yaw Y, pitch Z) *(start)*; a real car is not a uniform box |
| Wheels | `DynamicRayCastVehicleController`, 4 suspension mounts at (±1.3, 0.0, ±0.85), direction −Y, axle ±Z, radius 0.34 m, rest length 0.3 m, max travel 0.2 m *(start)*, with a bump stop (`bump_stop_start_m` 0.14 m, `bump_stop_rate_per_kg` 300: past that compression an extra spring joins, capped by `max_suspension_force_n`). This gives a centre of mass ≈ 0.6 m above the ground and ≈ 0.15 m ground clearance at rest |
| Suspension | stiffness, compression and relaxation damping, max force: tune for stable landings from 2 m drops |
| Drive | AWD; engine force up to 7.2 kN per wheel at low speed and at most 800 kW in total above that (force = power / speed, from about 28 m/s), about 0–100 km/h in 2.0 s and 0–200 km/h in 4.7 s; speed cap 103 m/s (371 km/h), reached in about 16 s; Boost fades out towards 110 m/s (research R2a, `docs/research/speed-survival.md`) *(start)* |
| Tyres | `friction_slip` 1.9: the 1.6 g launch needs it; a full-lock circle at 20 m/s peaks at about 1.57 g (1.3 gave 1.14 g) *(start)* |
| Continuous collision | `chassis.ccd_enabled` (on): Rapier CCD on the chassis, so a car at 100 m/s (0.83 m per step) cannot tunnel through a thin piece. The wheels are raycasts and do not depend on it: wheel contact at 100 m/s over floor seams and ramp kinks is tested with CCD off and on (`tests/roadster_drive.rs`) |
| Steering | max 0.5 rad at low speed, scaled down to 0.25 rad at 50 m/s; rate-limited to 3 rad/s |
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

- **Thruster interlock (M7):** while the car is steady on the ground (tilt ≤ 25°, centre of mass ≤ 1 m above the ground, |ω| ≤ 2.5 rad/s, |vertical speed| ≤ 3 m/s, wide enough that a wheel lifting on a bump or kerb still counts as on the ground; `[thrusters.interlock]` in vehicle.toml), the controller's throttles are cut to 0. Hop and Boost feed-forward still fire. No controller can idle thrust while parked or push the car around while driving, and no save is ever cut: a falling, tilted or spinning car isn't steady. `World::thrusters_locked()` reports it.
### 7.5 Air tank
- Mass flow per nozzle: `ṁ = F_actual / v_e`, with `v_e = 600 m/s` (cold-gas air). At full lift (48 kN) that is 80 kg/s.
- **Realistic:** capacity 260 kg (the reported figure), so full lift lasts about 3.25 s. Refill +20 kg/s while ≥ 3 wheels touch and no thruster has fired for 0.5 s (`refill_delay_s`); empty to full in about 13 s. This refill rate is a gameplay concession; label it so in the UI help text.
- **Empty tank:** thrust scales to 0 smoothly over the last 2% of air, so nothing pops.
- **Pressure falloff** *(v2, flag)*: `F_max · (0.5 + 0.5 · fill)`.
- **Arcade:** unlimited air. Still track "air used" for stats.

### 7.6 Modes
| Parameter | Realistic | Arcade |
|---|---|---|
| Thrust scale (× table 7.4) | 1.0 | 2.0 |
| Tank | 260 kg, refill 20 kg/s after 0.5 s idle | unlimited |
| Hop feed-forward | 0.75 on L_* for 0.40 s | 0.75 on L_* for 0.35 s |
| Hop apex, flat ground, controller Off (computed) | ≈ 1.7 m | ≈ 7.5 m |
| Hop air cost | ≈ 24 kg | n/a |
| Boost feed-forward | 1.0 on B_* while held | 1.0 on B_* while held |
| Boost extra acceleration | ≈ 0.9 g | ≈ 1.8 g |
| Hop cooldown | 1.5 s | 0.8 s |

Mode parameters are a struct `ModeParams { thrust_scale, unlimited_air, tank_capacity_kg, refill_kg_s, refill_delay_s, hop_ff, hop_s, hop_cooldown_s }`. Training samples it continuously (9.8), so a single policy serves both modes.

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
- **Boost** only fires while the button is held; it is allowed in the air too. A speed limiter fades its feed-forward to zero between 50 and 60 m/s of forward speed (`drive.boost_max_speed_m_s`, `boost_fade_m_s`), so Boost can't push the car to rocket speeds (Arcade's unlimited air passed 860 km/h without it). The controller's own use of the boost nozzles is not limited.

### 7.9 Events *(req definitions; start thresholds)*
- **Airborne start/end:** wheel-contact count goes to 0 (no wheel touches) / back above 0 (any wheel touches).
- **Upset start:** airborne for > 0.3 s, or tilt > 60°.
- **Save:** during an upset, 4 wheels touching, tilt < 15°, |ω| < 1.5 rad/s, held for 0.5 s. Emits `Save { duration, air_used, peak_tilt, landing, quality, peak_g }`: `landing` is the spin and fall speed just before the upset's first touchdown; `quality` (`nl_sim::landing`) is that touchdown's **heading error** (the angle between the nose flattened onto the ground at touchdown and at the last take-off, i.e. the last Airborne start; judged only when that take-off was from the wheels, so Stage A drops and Stage B tip-overs have none) and **speed kept** (forward speed 0.5 s after touchdown ÷ forward speed at take-off, only when the car left the ground from its wheels faster than 3 m/s, so drops and tip-overs are not judged on it; if the save comes before the 0.5 s, it is measured at the save). The take-off is forgotten once all four wheels are down again, so a later upset (e.g. tipped by a bump) is not judged against an old jump.
- **Hard landing:** within 0.2 s after a touchdown (first wheel down after none; the first step after the car is placed or respawned is never a touchdown), any wheel's suspension compression ≥ 0.95. Not a crash. Emits `HardLanding { compression, fall_speed }` once per touchdown (wasm `EventKind` 10). Thresholds in `vehicle.toml [events]`.
- **Crash:** any chassis or roof collider contact with relative normal speed > 6 m/s (`HardImpact`); **or** any roof contact while tilt > 120°, at any speed (`UpsideDown`); **or** stuck (tilt > 100° for > 2 s) (`Stuck`); **or** out of bounds (y < −20 or outside the course box) (`OutOfBounds`). Emits `Crash { cause }`.
- **Explosion and respawn (game only):** on `Crash` the sim freezes the wreck (no physics steps) and the game plays an explosion (flash, shockwave ring, about 20 low-poly debris pieces, camera shake unless reduced motion is on). After `respawn_delay_s` (1.5 s) of sim time (auto-respawn, which only the game turns on) the sim puts the car back at its last checkpoint (pose and velocities, not a `restore`) with a full tank and emits `Respawn`; mode, clock and random generator carry on. The HUD's "Lives used" counter is the crash count, which the game keeps as it drains events. The Reset button (R) respawns at once. In training a crash simply ends the episode (9.5).
- **Peak g (g-meter, `docs/contracts/g-meter.md`):** `EventTracker` keeps the raw (per-tick, unsmoothed) peak seat g over a stunt window that starts at `AirborneStart` and ends 0.5 s after the touchdown that ends the airtime (the touchdown counts). It resets at the next `AirborneStart`, a respawn and a placement, and is kept after the window closes. `Save.peak_g` carries it, `World::peak_g_load()` returns it (`HudField.PeakGLoad`, 23), and `World::peak_displayed_g_load()` returns the peak of the 0.1 s smoothed value over the same window. Nothing in the physics reads it.
- **Stunt events (game layer, wasm `EventKind` 11 to 14, five floats each; `docs/contracts/events-hud.md`):** `StuntStart [11, code, 0, 0, 0]`, `StuntLanded [12, code, peak_g, rotation_rad, 0]`, `StuntRefused [13, code, reason, 0, 0]`, `StuntAborted [14, code, reason, rotation_rad, 0]`. `code` is 1 barrel roll left, 2 barrel roll right, 3 front flip, 4 back flip; `reason` is 0 not enough air time, 1 landing blocked, 2 request expired, 3 control lost. They come from the stunt planner (11.8), not `nl-sim`, so the sim's event stream and the golden trace do not change.
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
- Piece types: `ramp`, `kicker` (a one-sided ramp that rolls the car), `corkscrew` (a twisted ramp that flips the car: `length`, `width`, `angle_deg`, `bank_deg`, `side`), `gap`, `car_row` (static boxes; `hop_hint` tells the driver bot to Hop), `halfpipe`, `bumps`, `kick_plate` (applies impulses in N·s and N·m·s on contact, once per pass), `wall`, `half_loop` (`radius`, `width`: a loop the car rides up and over, leaving upside down), `landing_pad`, `checkpoint`, `finish`.
- **Geometry** (`nl-sim/src/course_build.rs`, M7): Ramps, kickers and wall ramps curve up from flat to `angle_deg` at the lip in straight segments of rising slope (at least 6, enough that no two neighbours meet at more than 1.2°), built as one closed trimesh per piece with Rapier's internal-edge fix (separate convex segments stopped the car dead on their seams), so their height is below `length · tan(angle)`; a straight wedge's sharp foot crashed fast cars. `yaw` is in degrees and defaults to 0. The floor is the bounds' footprint with its top at y = 0; a `gap` cuts it across its whole width from `pos.x` to `pos.x + length` (gaps run along world X). Car-row boxes stand nose to tail along local +X. Pieces without a width use a 16 m lane. `bumps` are gentle humps `2 · height / 0.08` long (5 m for 0.2 m), one every length + 1 m, so the wheels lift the car before the chassis nose (0.14 m clearance) reaches them. A `kicker` runs from the lane's centre line out to `width` on its side, and its top slopes down to zero at the centre line, so only that side's wheels climb and no wall sits under the car. A `corkscrew` is centred on the lane, `width` across: its low edge curves up from flat to `angle_deg` at the lip (h = L·tan(angle)·s²/2, s = x/L), and the whole surface banks towards `side`'s raised edge by `bank_deg`·s², so it twists smoothly from flat at the foot to steepest at a sheer lip; one lofted trimesh sampled at 16 steps. Unaided, a car driven over a 15 m × 4 m, 15°, 45° corkscrew at 10–30 m/s leaves the lip banked and rolling and passes 90° of tilt (about 100–130° at 20–25 m/s with the entry speed held; at 15 m/s the 1.9 grip keeps the roll to about 66°); with PD the thrusters hold it near 50° and land it. A `wall` is 1 m thick; `with_ramp` makes it a table top of the wall's height instead (G2): a curved ramp of radius 110 m up to it, a 1 m deck and the same curve back down, starting at `pos` (the ramp's foot). A `gap` with `pit_depth` above 0 holds a catch pit (sloped floor from the lip's `entry_height`, 8° exit). `ramp` and `half_loop` take an optional `v_complete_min_m_s`. Curved pieces are laid out by `docs/contracts/ramp-curvature.md` (concave radius 110 m, lips up to 8°). Ramp, kicker and bump hulls reach 0.2 m below the floor, so even a nearly flat segment builds a solid collider. A `halfpipe` is a U-channel along +X: a 4 m flat floor between two quarter circles of `radius`, each wall one 0.3 m thick trimesh of 16 facets. A `half_loop` starts flush with the floor at `pos` and curves up along local +X and back over the entry as the inner face of a half circle of `radius` r about (0, r): a road `width` wide lofted in 48 facets as one 0.3 m thick trimesh, topping out at 2r straight above the entry. Its road is slick (friction 0.1, in `course_build.rs`): a circular loop pulls about 6 g, the suspension (0.2 m travel) bottoms out and the body scrapes, and at normal grip the car stopped on the wall and fell off. Unaided, a car at 17–24 m/s on a radius 5–6 m loop goes over the top past 160° of tilt and comes off heading back the way it came, upside down, then crashes; PD saves every pass but fires once the car passes the upset tilt on the wall, so it rides only to about 110–140° and uses about 90 kg of air. Obstacle colliders (car rows, bumps, walls) carry `OBSTACLE_USER_DATA`; a Stage C/D obstacle hit needs the chassis to touch one inside a row's zone. `landing_pad` (20 m), `kick_plate` (4 m), `checkpoint` and `finish` (0.5 m long, 20 m wide so a car can't drive around them) are flat markings the car drives through, not colliders. Leaving the bounds box with the centre of mass is an `OutOfBounds` crash.
- **Triggers** (`nl-sim/src/course_triggers.rs`, M7): each checkpoint, finish line and kick plate is a box standing on its marking, tested against the path of the car's centre of mass since the last physics step, so a fast car can't step over a 0.5 m line. Lines reach 30 m up, so jumping over one counts; kick plates reach 1.5 m. A trigger fires once, then re-arms only when the car leaves its footprint, so a car thrown up by a plate that lands back on it isn't kicked again. Teleports (respawn, place) re-arm every trigger. Kick impulses are in the world frame. Lines must sit on flat floor, because the respawn is level and 0.7 m above the line. Checkpoints count in file order, and a car can skip ahead but never go back. Reaching one moves the respawn point onto the line: upright, facing the line's yaw, at rest. Keeping the speed made a crash just past a checkpoint repeat forever. The finish counts once. Events: `CheckpointReached { number }`, `Finished { time_s }`, `Kicked { piece }` (wasm `EventKind` 7, 8, 9).
- **Checks** (`nl-sim/src/course.rs`): version 1 only; at most 200 pieces; every number finite; sizes 0.1–500 m; `count` 1–20; `angle_deg` in [1, 60); a corkscrew's `bank_deg` in [1, 80); bounds min < max and within ±5 km; the start and every piece `pos` inside the bounds; unknown fields and types are refused. Errors name the piece and field. `courses/gauntlet.json` is the source of truth for the Gauntlet; the JSON above is an example. `courses/arena.json` is the training viewer's obstacle arena (§11.6): four ramps facing outward, rows and single blocks, two bump strips and four low walls in a ring 6–32 m from the origin (pulled into the area where drops touch down, D1), around a clear drop zone (a landing pad marking) where Stage A drops start; bounds ±800 m, beyond the fog's 700 m end, so the floor's edge never shows. `Episode::start_on_course` places a scenario's car on a course in place of the course's own start.
- **Wasm and web** (M7): `Game.load_course(json)` starts a fresh run on a course and returns its shapes as JSON for the renderer; `Game.load_flat_ground()` goes back to the arena. HUD fields 16–19 hold checkpoints reached and total, finished, and finish time. In the game, Enter starts the Gauntlet, F returns to free driving, and `?course=gauntlet` opens straight into it. Free driving happens in the **playground** (`courses/playground.json`), an open world built from the piece types the AI trains on, plus one half loop (x 240, z 50, radius 6 m, with a landing pad facing back the way the car came; the training generator has no half loop yet): a jump boulevard along +X (ramps to landing pads, a car row to Hop, a kicker, bumps, kick plates, a wall jump), a skate park along +Z (four halfpipes of radius 5–8 m, kickers, wall jumps), a jump field along −X (ramps of 15°, 22° and 28° to landing pads, car rows of 1–2 m behind ramps), a bumps and kick-plate strip along −Z, a **flip zone** at x 140–440, z −100 to −220 (three corkscrews to landing pads, kick plates of 9 kN·s up with a 3 kN·m·s roll or a 7 kN·m·s nose-down pitch, each of which flips an unaided car, a row at z −190 with mild flip plates (barrel rolls, front flip, flat spin at strength 1) and a row at z −220 with strong ones (front and back flip, barrel rolls, roof toss at the strong strength), all driven along +X), and four free-roam ramps on the diagonals, around a clear spawn (no piece within about 55 m) with open floor between the zones; every piece stays over 300 m inside the bounds (±1500 m sideways, 300 m up for the centre of mass). `?world=flat` keeps the empty arena. The Rust eval runner takes `--course <json>` (`eval::run_episode_on_course`) to run any stage on a course. The results screen shows time, saves, lives used and air used.
- **Gauntlet** (the demo course; no longer in the game, kept for the sim, eval and tests), in order: start straight → kicker → gap → car row (Hop) → halfpipe (flip back) → bumps + kick plates → big ramp to a landing pad → wall with ramp → finish. A checkpoint after each section.
- **Procedural generator** (training): 4–8 pieces sampled from `pieces.json` with random parameters inside safe ranges, joined by straights of 20–40 m.
- **Generator notes** (`nl-sim/src/course_gen.rs`, `configs/pieces.json`, M7): courses run straight along +X on a 20 m track (bounds z ±10), start at rest at the origin. Each piece is a straight (at least 33 m before a jump: ramp, ramp + gap, wall), the piece, then its flat run-out (the landing zone); a checkpoint follows every run-out but the last, where the finish goes. Piece kinds: `ramp`, `ramp_gap` (a gap right after the lip, short enough to clear at 10 m/s), `kicker` (3.5–4.5 m wide, random side), `bumps`, `kick_plate` (up, sideways and a matching roll), `corkscrew` (weight 0.75, 13–16 m long, 3.5–4.5 m wide, 12–15° lip, 40–50° bank, random side, a jump with a 50 m run-out), `flip_plate` (weight 1, a jump with a 45 m run-out; see below), `halfpipe`, `wall` (weight 0 for now: its 30° ramp throws a car going over 20 m/s about 10 m up). No car rows: Stage C practises those. Draws use ChaCha8 stream 4 of the episode seed; every course passes the checks above (a test writes 300 to JSON and reads them back). The ranges are kept to what the driver bot with PD finishes (98 of 100 seeds).
- **Flip plates** (M7 wrap-up): a game device, not real physics. A `flip_plate` template in `pieces.json` lists named presets (front flip, back flip, barrel roll left and right, roof toss, flat spin), each a world-frame linear and angular impulse for a course along +X, scaled by a strength and picked by its `weight`; the generator writes it as an ordinary `kick_plate`, so the course format, loader and renderer are unchanged. Generated courses draw **mild** plates (strength 1.0–1.1): unaided they crash the car, but the thrusters catch the flip at 30–55° of tilt. The flat spin has weight 0 there (it lands the car 100–120° off course, which fails Stage D's heading rule). Stage D on 300 validation seeds: Off 7% (40% of episodes past 150°), PD 98% / 97%, Hybrid 97% / 96% (Realistic / Arcade). The playground also uses **strong** plates (`strong_strength`, 3): they turn the car past 120° even with the thrusters on, so Hybrid hands over to the AI. Tests: unaided, every preset crashes the car on at least 70% of passes at mild strength, every preset but the flat spin passes 150° on every pass at strong strength, and the flat spin always spins above 3 rad/s (`nl-sim/tests/flips.rs`); PD and Hybrid save every mild pass in both modes, and on strong plates every pass passes 120° with the AI taking over and Hybrid saves at least 90% (`nl-control/tests/flip_plates.rs`); every big kick plate on the hand-made courses is a preset at strength 1 or strong. About 78% of generated courses carry a corkscrew or flip plate (test: at least 60%). The eval summary's `inversion_rate` is the share of episodes whose peak tilt passed 150° (compare it between controllers on the same stage).

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

`act` takes only `ctx`: the driver's `InputFrame`, the car's `CarState` (the physical quantities the observation packs) and, from M5, the `observation` built from them (§9.2), which the AI reads. Every runner builds it with `ControlCtx::for_world`. Callers ask `World::is_control_step()` to run controllers at 60 Hz, and the inputs since the last control step are OR-ed so a quick tap is not lost.

1. **Off:** all zeros (feed-forward still works).
2. **Manual** (debug only, not in the T cycle): keys map to individual nozzles at throttle 1.
3. **Pd** (baseline):
   - **Attitude:** the desired up direction in the body frame is `up_des = −g_dir_body`; the error axis is `e = (0,1,0) × up_des`; the desired torque is `τ = I · (kp·e − kd·ω)`, with `I` the body-axis inertia. The gains follow the tilt: kp = 6.625, kd = 5.083 below 57.9°, kp = 39.46, kd = 4.532 past 83.0°, blended linearly between *(start; CMA-ES tuned 2026-10-27, `configs/controllers.toml`)*.
   - **Vertical, airborne and roughly upright (up·world_up > 0.7):** if downward speed > 4 m/s and height < max(3 m, 1.3 × the stopping distance + 1 m), command `F_y = m · (g + kv · (v_down − v_target))`, with kv = 6.025 and v_target = 0.254 m/s. The stopping distance is (v_down² − v_target²) / (2a), with a = 0.8 × full lift × the mode's thrust scale / m − g, so a fast fall brakes early enough to stop (a fixed 3 m start landed 10–15 m/s falls hard). Otherwise `F_y = 0` (saves air).
   - **Sideways, airborne and roughly upright:** `F_z = −m · kv_lateral · v_z` (body frame), kv_lateral = 0.93, so the side boosters cancel a slide before touchdown.
   - **Allocation:** solve `min ‖W(B·u − w)‖² + λ‖u‖²` subject to `0 ≤ u ≤ 1`, with `w = [0, F_y, 0, τx, τy, τz]` and `B` the 6×10 matrix scaled by max force. Use projected gradient descent (30 iterations, step `1/L`) or a small active-set NNLS. Torque weights are higher than force weights.
   - **Actuator-aware allocation (default, `[allocation] model = "actuator_aware"`, `lag_compensation = 0.357`, tuned with PD's gains; `"legacy"` keeps the old solve):** the same problem, but for the mean push over the next control interval as `nl_sim::actuator_model` predicts it from the valves, Hop/Boost throttles, tank and interlock: `B·u = (w/scale − B·(ū₀ − ū_ff)) / k`, where `ū₀` is the mean valve opening with no command, `ū_ff` the mean Hop/Boost throttle, `k` the share of a command change seen within the interval, and each `u_i ≤ 1 − max ū_ff,i`. While the interlock holds it plans nothing.
   - **Air control (round 7), airborne above `air_wish_min_height_m` (3 m) and upright:** the driver's keys steer the flight: steer asks for a yaw rate of ±`air_yaw_rate_rad_s` (1.5), throttle for a nose-down pitch of `air_pitch_rad` (0.35). Below that height, or tilted, PD ignores them and levels the car for the landing.
   - **Spin-through recovery:** when the car already spins toward upright fast enough to cover the long way round (ω²/(2α) > π − tilt), PD keeps the spin going instead of reversing it. The commanded angular acceleration is capped at `max_angular_accel_rad_s2` (25), which keeps the passenger g-load (9.4) moderate.
   - **On the ground and tipped over:** roll torque to flip back, plus a side-booster push along whichever body side axis points up, `ground_side_push` (0.4) × the car's weight.
   - Implementation notes (`nl-control/src/pd.rs`): PD is off while ≥ 3 wheels touch (it saves air); "on the ground" means below `on_ground_height_m`, not upright and nearly still, and there the roll torque is multiplied by `ground_roll_gain` (tipping the car over its side edge needs far more torque than turning it in the air); when nearly upside down the cross product vanishes, so PD rolls toward the lower side; gains live in `configs/controllers.toml`. PD reads `CarState` (`World::car_state`), the physical quantities the observation is built from.
4. **Ai:** loads `models/policy.json`, normalizes the observation, runs the MLP forward pass, and maps the action from [−1, 1] to [0, 1]. The policy is deterministic (mean action).
5. **Hybrid** (DECISIONS 2026-10-21): the AI saves upsets, PD does the rest. `nl_control::HybridGate` hands the nozzles to the AI when the tilt passes `events.upset_tilt_deg` (being airborne alone is not enough: PD lands upright jumps and hops; handing those to the AI too cut Stage D from 99% to 12%), and back to PD once all four wheels are down, tilt < `events.save_tilt_deg` and spin < `events.save_max_spin_rad_s` for `events.save_hold_s` (a latch, so a bounce never flicks control). The thresholds come from vehicle.toml `[events]`; an optional controllers.toml `[hybrid] enter_tilt_deg` moves only the hand-over tilt (120° in the repo since R4 shipped: the AI takes over only where PD struggles; EXPERIMENTS 2026-10-27). The game, both eval runners and training share the gate. **Training under the gate:** a training config with `controller: hybrid` runs PD hidden inside `BatchedSim` until the gate hands over; each AI stretch is one episode for the policy, ending at the gate's release (terminated), a crash or the time limit. PD stretches are never transitions, sim episodes PD flies alone are skipped, and checkpoints are scored by the Rust Hybrid eval. **Policy envelope** (plan G3, DECISIONS 2026-10-29): the policy was only validated up to 55 m/s and its observation clamps velocity at 60 m/s, so `nl_control::PolicyEnvelope` marks the car outside the envelope when the body forward speed exceeds `[hybrid] policy_max_speed_m_s` (default 55) or a saturating observation value sits on its clamp bound (angular and body velocity at either end, `thrust_scale` and `hop_time_left` at the top; flags, fractions, `height` and the terrain probes legitimately rest on their bounds and do not count). While outside, `HybridGate` gives the nozzles to PD whatever the latch says (the latch itself still runs by its normal rules, so the AI returns the moment the car is back inside), and `HudField.PolicyOutOfEnvelope` (27) is 1. Below 55 m/s and with nothing clamped nothing changes. "Validated" means someone ran the Hybrid eval in that region and logged it in docs/EXPERIMENTS.md; only then is the limit raised. A limit above 60 m/s cannot free the AI, because the body velocity is then clamped: that needs a new observation range (schema hash bump). The HUD shows "Hybrid · AI" or "Hybrid · PD"; T cycles Off → PD → AI → Hybrid. The game flies the Hybrid (PD + the shipped policy) by default (DECISIONS 2026-11-10: r5 gains 17 to 20 points on Stage F and is within about 1.5 points of PD elsewhere); it loads the policy and switches to Hybrid once it has loaded, staying on PD if the load fails. `?controller=hybrid` is the same as the default and `?controller=pd` keeps PD alone and skips loading the policy.

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
- Implementation notes (`crates/nl-py`): `config_json` holds the text of `vehicle.toml`, `modes.toml`, `scenarios.toml`, `reward.toml` and `controllers.toml` plus `mode` and `stage` (built by `ninelives.sim_config.load_sim_config`; the Rust module reads no files). Env `i` draws its episode seeds from ChaCha8 stream `i` of `seed`; `reset(seeds)` overrides them once. `info` holds `done` (rows whose episode ended), `terminal_obs`, `episode` (per row: the finished episode as a JSON object string with the eval `EpisodeResult` fields, so `success`, `saves`, `crash_cause`, `air_used_kg` and the touchdown numbers, plus `stage`, `level` and `control_steps`; None elsewhere) and `reward_terms` (N × 7, columns from `reward_term_names()`). The episode row replaces separate `saves`/`crashes`/… arrays so its field names match the results files. `set_stage_and_level(stage, level)` applies from each env's next episode.

`ninelives.env`:
- `NineLivesVecEnv(sb3.common.vec_env.VecEnv)` adapts `BatchedSim` to SB3. It sets `infos[i]["terminal_observation"]` and `infos[i]["TimeLimit.truncated"]`.
- `NineLivesEnv(gymnasium.Env)` is a single-env wrapper, used only for `gymnasium.utils.env_checker.check_env` and debugging.

### 9.2 Observation (OBS_DIM = 43, v2) *(req layout)*
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
| 41–42 | heading_error | sin and cos of the nose's signed yaw from the take-off heading (positive: turned left, about world +Y); both 0 with no heading to keep (no take-off from the wheels, or the nose straight up or down). v2, DECISIONS 2026-10-27 |

The layout lives in Rust (`ObsSpec`). The schema hash covers names, order and clamp ranges. Python, the export and the browser all check the hash *(req)*.

Implementation notes (`nl-sim/src/obs.rs`): every value is clamped to the range listed in `OBS_SPEC` (spin ±30 rad/s, velocity ±60 m/s, thrust scale 0..3, Hop time 0..1 s), so the ranges double as the Gymnasium `Box` bounds. `prev_action` holds the controller throttles in [0, 1], not the policy's [−1, 1] actions. Each terrain probe is a world-down ray from 20 m above the centre of mass at that distance along the nose direction flattened onto the ground (world +X when the nose points straight up or down); a probe that hits nothing reads −20. Suspension compression is (rest length − measured length) / max travel, 0 when the wheel does not touch. The schema hash is FNV-1a 64 over each field's `name low high` line in order, as 16 hex digits (v1: `16d2f5faa7964d05`, 41 values; v2: `9cfb7c4a8a03a228`); it covers the clamp ranges as well as names and order, so a policy never runs on inputs clamped differently from training. Python and the browser read the hash from Rust rather than recomputing it.

### 9.3 Action
`Box(low=−1, high=1, shape=(10,))`, mapped in the sim to `u = (a + 1) / 2`.

### 9.4 Reward (per control step; weights in `configs/reward.toml`) *(start)*
| Term | Formula | Weight |
|---|---|---|
| Tilt | `(up·world_up − 1)` | 0.05 |
| Spin | `−min(‖ω‖², 50)` | 0.002 |
| Air (limited) | `−air_used_step / tank_capacity` | 8.0 (was 2.0; see DECISIONS) |
| Air (unlimited) | `−Σ u` | 0.008 (was 0.002) |
| Airborne time | `−1` per step while airborne for longer than 0.5 s | 0.02 |
| Smoothness | `−‖u_t − u_{t−1}‖²` | 0.002 |
| Save (event) | `2 + 3·q`, `q = clamp(1 − ‖ω‖/4) · clamp(1 − v_down/6)`, with ‖ω‖ and v_down taken just before the touchdown that led to the save | 1 |
| Crash (event, terminal) | `−10` | 1 |
| Hard landing (event) | `−1` per `HardLanding` | 1 |
| Unsettled | `−1` per step after the episode's first touchdown while not settled (9.5) | 0.01 |
| Air left (success) | share of the tank left, once, on the success step; limited tank only | 1 |
| G-force | `−max(0, g_seat − 3)²` with `g_seat` the passenger g-load: the smoothed (0.1 s) specific force at the driver's seat (`[passenger]` in vehicle.toml; `World::g_load()`) | 0.05 |
| Success (event, terminal) | `+1` once, on the step the episode succeeds (9.5); success needs the quiet hold, so this pays for a quiet landing | 5 |

The save quality also has two landing factors (PLAN §3.3): `q = clamp(1 − ‖ω‖/4) · clamp(1 − v_down/6) · clamp(1 − heading_error/45°) · clamp(speed_kept)`; an unknown heading error or speed kept (7.9) counts as 1, so both factors apply only after a take-off from the wheels (drops and tip-overs are unaffected).

There is no survival bonus *(req)*. Being alive must not pay, or the policy learns to hover (9.11).

### 9.5 Episode end
- **Terminated:** crash; or success. Stage A/B: all four wheels down for 1.0 s unbroken **and** the controller's throttle sum < 0.05 for the last 0.5 s, unbroken — a clean landing counts even if no upset, and so no Save event, happened; idle thrust while parked is not a finished episode. Stage C: a save after the hop, with no touch of the car row. Stage D: crossing the finish line. Stage C and D also need every judged landing's heading error below `episode.max_landing_heading_error_deg` (30°, PLAN §3.3): a save whose landing turned further fails the episode at once (`EpisodeStatus::BadLanding`). Stage F: four wheels down for 1.0 s with quiet throttles, as Stage A, and the landing heading within the Stage C/D limit. Stage E: the base stage's success after every knock, and on A or C four wheels down for 1.0 s with quiet throttles after the last knock. Implemented once in `nl_sim::episode::Episode`, which also applies the driver bot's input and the knocks inside its step; thresholds in `scenarios.toml [episode]`.
- **Settled** (PLAN §3.3): four wheels down, |ω| below the save rule's 1.5 rad/s (`events.save_max_spin_rad_s`) and sideways speed < 1 m/s. **Settle time** runs from the first touchdown to the start of the settled run the episode ends in (none on a crash). `Episode::last_save_quality()` exposes the latest save's heading error, which the Stage C/D rule reads.
- **Touching the row** (Stage C): the body or roof touching anything while the car's centre is within the row's footprint grown by the car's half-size (less 0.2 m). Wheel contact doesn't count: the suspension rays reach below the wheels, so a hop clearing the roofs by a few tenths of a metre would read as a hit. A hit ends the episode at once as `EpisodeStatus::HitObstacle` (terminated, not a crash, no crash penalty).
- Tracks and target speeds are sized by the time limit of the stage actually running (Stage E's 45 s when E builds on C or D).
- **Truncated:** time limit (A/B 8 s, C 15 s, D/E 45 s, F 10 s).

### 9.6 Scenario stages
| Stage | Spawn | Driver | Success |
|---|---|---|---|
| A — Drop | height U(1.5, 8) m (raised if a tilted corner would be closer than `spawn_clearance_m` to the ground); tilt U(0, θmax) about a random horizontal axis; ω magnitude U(0, ωmax) on a random axis; horizontal v U(0, 15); vertical v U(−5, 3) | none | save |
| B — Side tip-over | resting on its side at low speed (lowest corner `spawn_clearance_m` above the ground), roll U(60°, 110°) to either side, small pitch and yaw | none | back on 4 wheels, upright, without touching the roof |
| C — Hop | driving straight at U(5, 25) m/s on flat ground; Hop at a random time; in 50% of episodes a car row of 1–3 cars (1.2–1.5 m tall) placed so the hop must clear it | bot | save after the hop, no obstacle hit |
| D — Course | procedural course (7.11) | bot | reach the finish with no crash |
| E — Chaos | any of A, C, D plus 1–3 Chaos impulses at random times | bot or none | as for the base stage |
| F — Flight | driving at U(10, 28) m/s, 5 m before a **strong** flip plate (§7.11; preset by weight, flat spin excluded) at strength U(2, 3); a 300 m run-out | bot | land and hold: as Stage A, with the landing heading judged |

Curriculum for A: θmax goes 30° → 180° and ωmax 0 → 6 rad/s, in 5 levels. The levels live in `[curriculum_a]` of `configs/scenarios.toml`; the last level is the full `[stage_a]`, which eval always uses. A level change applies from each env's next episode.

### 9.7 Driver bot (training only)
- Pure-pursuit steering on the course centerline: lookahead `6 + 0.3·v` m; P control on speed toward a target U(10, 30) m/s.
- Steering noise: an OU process (σ = 0.1). There is a 5% chance per second of a random steering jerk of ±0.5 for 0.3 s.
- Presses Hop at `hop_hint` pieces with ±0.15 s timing noise. Never uses Manual thrusters.
- Implementation notes (`nl-sim/src/driver_bot.rs`, `[driver_bot]` in scenarios.toml): the centre line is the course start's heading (courses are straight). The pursuit's turn is capped so it pulls at most `max_turn_accel_m_s2` (6 m/s²) sideways, and the wobble (σ) and jerks (±0.5) are fractions of that grip-safe turn, so they are the same size at any speed; uncapped, the bot spun the car whenever it landed off the line. In the air it centres the steering. Hop is timed from a 1-D hop prediction (`hop_profile.rs`, which errs about 0.3 m low) so the wheels are highest over the row's middle, and is re-tapped until a Hop starts. Stage C holds its start speed; on Stage D the target is raised to at least the course length ÷ (0.75 × time limit). Randomness: ChaCha8 stream 5 (target speed 7) of the episode seed.
- **Stage C notes** (`driving_stages.rs`): the track is flat, straight and 20 m wide. A row (50%) is placed so a Hop pressed at the drawn time U(1, 3) s is highest over its middle, and keeps only as many cars as that hop clears at the start speed (wheels 0.1 m above the roofs for the row plus the car's length plus 1 m); if none fit, the episode has no row. Without a row the bot presses Hop at the drawn time.
- **Stage E notes:** the base stage (A, C, D, weights `[stage_e] base_stage_weights`) and 1–3 knock times U(0.5, 6) s come from ChaCha8 stream 6; a knock is the Chaos button on that physics step.

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
| Wind | strength U(0, 1) × the Storm level (mean speed and gust size), random direction |
| Thrust scale | U(1.0, 2.0) |
| Unlimited air | probability 0.4; otherwise tank U(150, 350) kg |

Per-mode policies (round 7): a train config with `randomize_mode: false` (scenarios `vary_mode = false`) keeps its `mode` preset (thrust scale, air, tank) and randomizes everything else; the mode is still drawn and thrown away, so the other draws match. `eval --dr --keep-mode` validates such a policy. Historical: round 7 trained one policy per mode (`models/policy-realistic.json`, `models/policy-arcade.json`); those files and their configs were dropped, and the shipped game uses the one `models/policy.json`.

Implementation notes (`nl-sim/src/randomization.rs`): the ranges live in `[randomization]` of `configs/scenarios.toml`; each episode draws them from its own seed on ChaCha8 stream 2 (noise during the episode uses stream 3), so a seed always gives the same car. The inertia factor is one draw for all three axes; the force factor is one draw per nozzle. The centre-of-mass offset (`chassis.center_of_mass_offset_m`) is a vehicle.toml field that defaults to 0. Wind is `randomization.wind_strength` × `[wind.storm]` with gusts, in a random direction (`EpisodeVariation::wind`); `none` is calm. `Episode` applies a variation (`EpisodeVariation`, or `EpisodeVariation::none`): the varied car, mode, ground and wind at the start, then latency (the valves get the throttles of `latency` control steps ago) and observation noise (`Episode::observe`). The observation's `prev_action` shows what the controller commanded, the same signal the smoothness cost uses. Training turns randomization on (`randomize: true` in the training YAML); eval leaves it off unless `--dr` is given (results get a `_dr` suffix); the game leaves it off.

### 9.9 Curriculum callback
- Track the success rate over the last 500 episodes per stage. When it exceeds 85%, advance that stage's level or unlock the next stage.
- Once unlocked, sample stages with weights A 0.25, B 0.15, C 0.2, D 0.3, E 0.1, so earlier skills are not forgotten.
- Log the stage, level and success rates to TensorBoard.
- Implementation notes: `BatchedSim.set_stage_mix([(stage, weight, level), …])`; each episode picks its stage by weight from its own seed (ChaCha8 stream 8), so the same seed always runs the same stage. The training YAML lists `stages` in unlock order and `curriculum.stage_weights`; the Python curriculum (`ninelives/curriculum.py`) keeps one 500-episode window per stage at its level, raises the level above 85%, and unlocks the next stage when the newest one passes at its last level. Every stage needs a weight above 0 (a zero-weight stage would unlock but never run, and block the stages after it). "Best" is the eval success over the episodes of the stages unlocked at that checkpoint, each at its hardest level; the log line names them, and when a new stage unlocks the best score restarts (scores over different stage sets don't compare). `pieces.json` reaches `BatchedSim` with the TOMLs (`pieces_json` in the config JSON; the built-in copy if absent), so a run's saved `configs/` are what it trained on.

- **Promotion by eval (M7):** with `curriculum.promote_on: eval` (the default in `configs/train/base.yaml`), each 1M-step checkpoint promotes a stage whose deterministic eval at its current level (the fixed eval seeds) passes the bar. The training window is only logged, because exploration noise alone breaks the quiet-throttle success rule.
- **Starting policy:** `ppo.initial_action_bias: -1` and `ppo.log_std_init: 0`, so each throttle's mean starts at the edge of zero.

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
For the training viewer (§11.6), training also exports up to 8 evenly spaced checkpoints of the run behind `models/policy.json` in this format to `web/public/checkpoints/<run-id>/<n>.json` (weights rounded to 4 significant digits, under 3 MB in total). Those few are committed, like `models/policy.json`; every other checkpoint stays in `runs/` (gitignored, ADR-003).
```json
{
  "format": "ninelives-policy", "version": 1,
  "obs_schema_hash": "…", "obs_dim": 43, "act_dim": 10,
  "obs_norm": { "mean": [], "var": [], "eps": 1e-8, "clip": 10.0 },
  "layers": [ { "w": [], "b": [], "in": 43, "out": 128, "act": "tanh" },
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

- The Rust runner (M4, no Python): `cargo run --release -p nl-control --example eval -- --controller {off,pd,hybrid,ai} --stage {A,B,C,D,E,F} --mode {realistic,arcade} --episodes 1000 --seed 0`. Both runners step `nl_sim::episode::Episode`, so success, crash and time limit mean the same thing.
- `python -m ninelives.eval --controller {off,pd,ai,hybrid} --stage {A,B,C,D,E,F} --mode {realistic,arcade} --episodes 1000 --seed 0`. It runs the episodes in Rust (`ninelives_sim.run_eval`, the same `run_episode` and summary as the Rust runner), so the two runners give identical rows; the Rust runner also takes `--controller ai --policy <file>`.
- A fixed seed list makes runs reproducible. Domain randomization is off by default in eval (`--dr` turns it on).
- **Outputs:** `results/eval_<controller>_<stage>_<mode>.json` (no date: a rerun overwrites the file and git shows the change; the M5 Python eval adds a `.csv`), with per-episode rows (success, saves, crashes, cause, air used, peak tilt, touchdown |ω|, touchdown v_down, hard landings, landing heading error of the latest save, speed kept, settle time, air left — null with unlimited air). Every file also records `physics_hash` (the vehicle, modes, pieces and approach files), `controller_config_hash` (controllers.toml), `policy_hash` (null without a policy) and `git_sha` (with `-dirty` if configs or code differ from it), computed by `nl_sim::provenance` so the Rust and Python evals agree; `--out <dir>` picks the folder.
- **Metrics:** save rate, crash rate, air used per save (kg), touchdown spin and vertical speed (median and p95), hard-landing rate, median landing heading error, speed kept and settle time, median air left at success, and Gauntlet completion rate with the driver bot.
- **Charts** (`python -m ninelives.charts`, SVG into `results/`): save rate by stage for Off/PD/AI; air per save, PD vs AI; touchdown spin distributions; the training curve.

**Targets** *(start; revise after first results; report honestly either way)*:
- Stage A Arcade, AI save rate ≥ 95%; Stage A Realistic ≥ 80%; Stage B Arcade ≥ 90%.
- AI beats PD on save rate or on air per save in Realistic mode.
- Gauntlet crash rate with AI ≤ ⅓ of the crash rate with Off. (Historical: the Gauntlet is cut from the game, DECISIONS 2026-11-03; the course stays for the sim and eval.)
- **M7 bar** (2026-10-21): the AI, run as `Hybrid` (it owns the nozzles only during upsets; PD otherwise), beats PD on Stages A, E and F and stays within PD's 95% confidence interval where PD succeeds ≥ 97% (B, C, D), in both modes. Comparisons use the same seeds for both controllers and ≥ 1000 episodes.
- **Seed blocks** (wrap-up, 2026-10-21): 0–999 were the M7 test seeds and are spent; validation (checkpoint choice, tuning) uses 1,000,000 and up, below 3,000,000; the final one-time test uses 3,000,000–3,000,999. `tune_pd.py` refuses tuning seeds in the test block.

---

## 11. Web game (`web/` + `nl-wasm`)

### 11.1 Wasm API (`nl-wasm`)
```ts
class Game {
  constructor(vehicleToml: string, modesToml: string, controllersToml: string, seed: bigint);  // courseJson joins in M6
  set_mode(mode: "realistic" | "arcade"): void;
  set_controller(kind: "off" | "pd" | "ai" | "manual"): void;
  load_policy(json: string): void;          // throws if obs_schema_hash mismatches
  tick(input: Uint8Array /* packed InputFrame */): void;  // advances exactly one physics step
  chassis_pose(): Float32Array;             // [px,py,pz,qx,qy,qz,qw]
  wheel_poses(): Float32Array;              // 4 × 7
  nozzle_throttles(): Float32Array;         // 10 flame sizes: valve opening × tank gate
  nozzle_layout(): Float32Array;            // 10 × [x, y, z, exhaust x, y, z] in the body frame
  hud(): Float32Array;                      // fixed layout: speed, air_frac, tilt, |ω|, saves, crashes, time, hop_cd, …
  drain_events(): Float32Array;             // 5 per event: EventKind, then up to 4 values (Save: duration, air used, peak tilt, peak g; CrashCode); the rest 0
  is_crashed(): boolean;
  set_checkpoint(): void;                   // the car's pose and motion now become the respawn point
  snapshot(): Uint8Array; restore(s: Uint8Array): void;
}
```
`InputFrame`: throttle (i8), steer (i8), bit flags (hop, boost, chaos, reset, manual nozzle bits).

### 11.2 Main loop
- `requestAnimationFrame` with a fixed-step accumulator: `tick()` at 120 Hz, at most 8 ticks per frame (drop time beyond that). The renderer interpolates between the last two poses.
- The policy runs inside wasm (no JS inference).

### 11.3 Controls
Shown in the key card (top-right, folded to an "H keys" chip; the chip or H unfolds it): up/down drive, left/right steer, Space hop, Shift boost, A S barrel roll, D F flip, X drop test, M air mode, V wind, Esc pause.

| Key | Action |
|---|---|
| ↑ ↓ | throttle, brake / reverse (arrows only; W A S D do not drive) |
| ← → | steer |
| Space | Hop |
| Shift | Boost |
| A / S | request a barrel roll left / right (11.8) |
| D / F | request a front flip / back flip (11.8) |
| X | Drop test: throw the car into a random Stage A start (§9.6), to watch the controller save it |
| R | respawn at the last checkpoint (the `reset` bit in the InputFrame); works but is not listed in the key card |
| M | toggle Limited air / Unlimited air (`realistic` / `arcade`) |
| V | cycle wind: Off → Breeze → Storm (`?wind=breeze` or `?wind=storm` picks the starting level) |
| Esc | pause |
| H | fold or unfold the key card |
| F3 | frame-rate overlay |
| 1–0 | fire nozzles 0–9 (only when the controller is Manual, set through the API) |

The game flies the Hybrid (PD + AI) by default and switches to it once the policy has loaded (`?controller=pd` flies PD alone and skips loading the policy); there is no key to change the controller.

### 11.4 Visuals
- Low-poly generic sports car (no brand shapes) in a **lab arena**: a blue-grey checkered floor that fades into a dark sky dome (its horizon is the fog colour, so there is no edge), neutral light, soft shadows from one directional light, and plain-coloured, flat-shaded course pieces (ramps, blocks), like a physics simulator's test arena.
- **Nozzles:** discreet flush ports on the body surface nearest each 7.4 position (side nozzles on the flanks under the beltline, lift nozzles under the floor clear of the wheel arches, rear nozzles in the tail like exhaust tips; `web/src/render/nozzle-ports.ts`), at most about 0.4 m from where the force acts. Each flame is an additive-blended cone mesh whose length and opacity follow its effective throttle, plus a light puff of particles. Colour is a cold blue-white (cold gas), never fire.
- **Camera:** smoothed chase camera that stays world-up when the car flips, so the player sees the flip. A slight pull-back while airborne.
- Respect `prefers-reduced-motion`: no camera shake, fewer particles.

### 11.5 HUD
Liquid-glass look (owner's choice from a mock, DECISIONS 2026-11-10). Every readout on the game page is a see-through glass panel (`.glass` in `web/src/ui/theme.css`): ink at 35% tint (`--glass-tint`) over a faint top highlight, a 1 px light edge, an inner top highlight, a soft shadow, rounded corners (18 px) and `backdrop-filter: blur(8px) saturate(1.6)`. In Chromium an inline SVG displacement filter adds real refraction (`data-refract="on"` on `<html>`, set by `web/src/ui/refraction.ts` after feature detection); other browsers show the blur. Under `prefers-reduced-transparency: reduce` the panels fall back to the opaque ink look (flat slate, thin edge, square corners). The training viewer keeps ink panels (`.tile`). Barlow and Barlow Condensed, self-hosted; one rust accent, the car's colour in the scene; red only for danger.
- **Type:** five sizes, 13 / 15 / 20 / 32 / 56 px, nothing smaller. Barlow Condensed 700 for numbers only, Barlow Condensed 600 uppercase for labels, Barlow 400 / 600 for sentences. Three letter-spacings only: 0, 0.04em, 0.1em. Every label is at most six words.
- **Centre instruments** (desktop, keyboard; the phone layout is §11.9):
  - **Top-left:** the nine hearts and the saves counter ("3 saves", this run's saves, live).
  - **Top-centre:** the message stack, off the road.
  - **Top-right:** the key card (11.3), folded to an "H keys" chip on every screen size; a click on the chip or the H key unfolds it. There is no viewer link in the driving HUD (the title and pause screens keep "Watch the AI train").
  - **Bottom-centre:** the dash, about 380 px wide: speed in km/h at 56 px (rust above 200 km/h) and the air bar, 240 px wide (sand; ∞ with unlimited air; rust and flashing below 15%). The nozzle map sits to its left and the g-meter to its right, in fixed places, so the dash never moves when they fade in.
  - **Bottom-left:** the air mode ("Limited air" or "Unlimited air", with the course section, e.g. "Checkpoint 2/6"). **Bottom-right:** the wind arrow with Off / Breeze / Storm ("Off" with no wind). Both readouts are buttons: a click or tap does what M and V do.
- **Nozzle map and g-meter:** shown whenever any nozzle fires (the automatic saves are the point of the game), while a thruster key (1 to 0) is held, or after a hard landing (3 g or more), then fade within about a second. Under the map, "AI flying" while the trained policy is in control (`HudField.Controller` is `Ai` or `HybridAi`), otherwise "Thrusters". There is no stunt-ready tile.
- **G-meter:** the live passenger g (`HudField.GLoad`, the 0.1 s smoothed value) and the headline peak of the stunt window (the smoothed peak capped by the raw `HudField.PeakGLoad`, which shows in the tooltip); sand, rust from 3 g, red from 6 g. The g numbers stay on the meter; toasts carry none. The old best-g record line is gone.
- **Messages (top-centre), one stack of glass panels:** one short toast after a jump, about 1 s ("landed", or "back flip", "front flip", "barrel roll"; "crashed: roof", "crashed: too hard", "crashed: on its side" and "no air" in red; "back in bounds"), the approach hint ("need N km/h") and "too slow", the Hop note ("wheels down first", "no air to hop", "hop ready in 1 s"), and an "AI unsure" badge while `PolicyOutOfEnvelope` is 1. Scrapes throw sparks.

### 11.6 Screens and features
- **Title screen:** one line ("Jump, flip, land on your wheels. Nine tries."), Play, and "Watch the AI train". The game's clock waits behind it; `?play` skips it. (Mode and controller pickers are not built: M toggles the mode, the game flies the Hybrid by default, `?controller=pd` for PD alone.)
- **Pause** (Esc): a ticket with the way to the training viewer. **Game over** (all nine hearts lost): a plain page, "Out of lives - N saves", and a Restart button; no stats list. **Out of bounds** is a free respawn: the game layer (`web/src/game/out-of-bounds.ts`) gives the life back, shows "back in bounds" and skips the explosion; the sim still counts the crash, so training evals are unchanged. The Gauntlet is not part of the game (DECISIONS 2026-11-03). The results screen and the ghost run were removed (DECISIONS 2026-11-05).
- **Training viewer** (`viewer.html`): watch a crowd of 64 translucent cars fly the same 64 episodes (eval seeds 0–63) with checkpoint 1, 2, 3… of the newest exported run, auto-advancing, with each checkpoint's landed, crashed and still-flying counts (landed: the episode succeeded, or on a drop ran out of time after a save without crashing, so a policy that still blows air while parked shows its landings). One wasm `Crowd` per checkpoint flies every car as a full `Episode`, and **every car is flown by the Hybrid** (`build_controller("hybrid", …)`, the hand-over tilt from `controllers.toml`) with the checkpoint as its AI, as the game and the eval run it: PD flies until the car tips past the hand-over, so the viewer shows what the policy adds to PD. `ninelives.export --viewer` writes the checkpoint files plus `checkpoints/index.json`. `Crowd.for_stage(policy, mode, stage, first_seed, count)` covers five stages: `A` (Stage A drops in the flat arena, M6), `arena` (Stage A drops in the **obstacle arena**, M7, `courses/arena.json`), `D` (the driver bot on the generated course of `first_seed`), `F` (the bot rolling onto the flip plate of a generated straight) and `gauntlet` (the Stage D bot on `courses/gauntlet.json`; the viewer does not show it). In D and F the cars of a crowd share the course of the first seed, so the viewer draws one course (`episode::course_for_stage(variation, scenarios, stage, seed)` returns the course a stage runs on, or none on flat ground); their seeds vary the driver's speed. The viewer shows every stage at once (see *All stages at once* below) and `?arena=flat|side|obstacles|driving|flight` focuses one; `Crowd.on_course(policy, mode, first_seed, count, course_json)` drops Stage A cars onto any course. `Crowd.course_shapes()` hands the viewer the shapes, drawn by the game's course renderer with the endless floor hidden; on the courses the camera follows the cars. The obstacle arena's pieces sit within about 40 m of the centre (ramp feet 6 m out, walls 31.5 m), inside the area where Stage A drops first touch ground (90% within 15 m, farthest 31 m of 256 seeds), so drops meet them. Cars share the one arena, so they are drawn at their true positions, not on the grid. The viewer landing rates under the current physics are recorded in `results/baseline_2026-10/viewer_landing_rates.json` (`cargo run --release -p nl-wasm --example viewer_rates`). The game links to it from the title screen and the pause menu; the viewer's "back to game" link returns. The viewer's status line reads "8 checkpoints, 5 stages", panels show "12/16 landed", and the camera button "camera: crowd"; the two help paragraphs are gone.

- **Training viewer additions (G5):** a chart of `web/public/checkpoints/<run>/curve.json`, titled exactly **"Checkpoint evaluation under current physics"** in `curve.json` (the page shows the shorter label "Checkpoint results"): the 8 committed checkpoints flown under the Hybrid in the current physics (`python -m ninelives.export --viewer-curve`, 200 episodes per stage, Realistic, seeds from 0), with the physics, controller-config, git and per-checkpoint policy hashes in the file. It is not a historical training curve. A to F stage tabs choose the stage the chart shows; a leader camera (key L, a button or `?camera=leader`) follows the furthest-along car on the course arenas (D, F); and committed showcase seeds (`showcase.json`, chosen by `web/scripts/choose-showcase-seeds.mjs`) give each tab with a shared-seed crowd (A, D, F) a reproducible drop. Stages B, C and E have no crowd binding, so those tabs show the curve only. If the checkpoints change, re-run the export and the chooser.

- **Training viewer, all stages at once (2026-11):** the viewer flies Stage A (flat arena), Stage B (cars on their side), the obstacle arena, Stage D and Stage F **at the same time**, 8 checkpoints × 8 drops each (320 cars), without dropping frames. One canvas, one renderer and one scene; each stage is drawn into its own rectangle (a scissor viewport) with its own camera and its own pool of instanced car meshes, on its own three.js layer. Each stage is stepped by **its own Web Worker** (`stage-worker.ts`; one wasm instance per stage, real time at 120 Hz, a new round 2.5 s after the last drop ends) that posts the cars' poses every 8 ms of real time as transferable buffers, each snapshot stamped with the simulated time; the page blends between the last two snapshots at a display time that runs at real speed (`snapshot-clock.ts`), so the main thread does no physics. Panels tile the area left of the side column (three drop arenas on top, Stage D twice as wide as Stage F below); a tab, a click on a panel's title, the keys 1–5 or `?arena=` focuses one stage (the others stop stepping), and the All tab, Esc or 0 tile them again. Tiled panels label one car per checkpoint row on the drop grids and none on the courses; a focused panel labels every car. The side column holds the status line, the tabs, the camera button, the curve, the checkpoint × stage board (landed / crashed) and a link back to the game. F3 shows fps, the main thread's ms per frame and the workers' summed ms per tick, with each stage's cost on a second line. The viewer draws no shadows (the sun's shadow box covers 30 m around the origin and the crowd casts none). The Gauntlet is not shown anywhere in the viewer.

### 11.7 Performance and deploy
- Target 60 fps on a mid-range laptop. Budget: sim tick ≤ 1.5 ms; render ≤ 8 ms. Measure and show an optional FPS / ms overlay (F3).
- Release wasm built with `opt-level = "s"` or `"z"`, LTO and `wasm-opt`. Target wasm ≤ 2.5 MB gzipped.
- Static deploy on Railway: a small static file server container serves `web/dist` plus `models/policy.json` plus `courses/` plus `results/*.svg` (with the `application/wasm` MIME type). No game server or API.

### 11.8 Stunts
A stunt is a request, never a trigger: the player asks, the planner decides, and the car rotates only if it can also land.
- **Keys:** A / S barrel roll left / right, D front flip, F back flip. A press queues one request (a newer press replaces it) and expires after 3 s (`[stunt] request_timeout_s`). Presses are ignored while paused or after game over.
- **Planner** (`nl_control::stunt::StuntPlanner`, settings in `configs/controllers.toml [stunt]`, research in `docs/research/stunt-planning.md`): authorizes only if all hold: (1) the landing surface normal is within 15° of up and the usable length is at least 2 car lengths; (2) `t_avail − uncertainty ≥ t_spin + t_catch + t_settle_margin`; (3) the tank holds spin-up, braking, catch and a landing reserve with a 1.25 air margin; (4) the car is airborne, inside the start envelope (tilt ≤ 25°, spin ≤ 1.5 rad/s), with a full tank gate and a known mode. Otherwise it emits `StuntRefused` with the first rule that failed.
- **Phases** (`docs/contracts/stunt-states.md`): Idle, Requested, Authorized, SpinUp, Coast, Brake, Catch, Settled, Aborted (HUD `StuntPhase` 0 to 8). It replans every control tick and aborts to PD's Catch, emitting `StuntAborted`, if the landing is lost, tilted or blocked, thrust is lost, a wheel touches early, or the window shrinks below the catch time.
- **Complete** means the integrated rotation about the stunt axis reaches at least 2π − 0.15 rad; being upright at the end is not enough (an upright landing with a short rotation is an abort, never `StuntLanded`).
- **Executor** (`StuntDirector`): spin and brake commands become throttles through PD's allocation (`Pd::spin_throttles`), and PD flies the catch, so exactly one owner writes the nozzles each tick. The Hybrid gate stays on PD from authorization until 1 s after Settled. With no stunt requested the director does nothing and the active controller flies as before.
- **Toasts:** "back flip" / "front flip" / "barrel roll" on a landed stunt (no g number; the meter shows g, 7.9), "not enough air", "landing blocked", "stunt aborted" (or "landing blocked" when the landing was the cause). A lapsed request is silent.
- **Evidence:** `crates/nl-control/tests/stunt_flight.rs` flies all four stunts on a real car from 60 m at 15 m/s: each completes 6.25 to 6.30 rad, lands on four wheels and does not crash.
### 11.9 Phones and gamepads
Touch controls show only on touch devices: `(pointer: coarse)` matches, or after the first `touchstart` (`web/src/ui/touch-mode.ts` sets `data-touch="on"` on `<html>`). Mouse and keyboard users never see them. The key chip and key card are hidden on touch. The sim and the wasm API are unchanged: throttle and steer are already signed bytes, so the stick and triggers send analog values; `web/src/ui/input-merge.ts` merges keyboard, touch and gamepad into the one 4-byte InputFrame (each axis takes the source pushing hardest, buttons are OR-ed).
- **Stick:** one floating joystick. A touch in the left 45% of the screen (below the top row) puts the ring under the finger; the knob follows, clamped to the ring (travel 0.6 of its radius, dead zone 0.12); release eases it home. Up is gas, down is brake or reverse, left and right steer. The game surface has `touch-action: none`.
- **Buttons:** HOP (about 88 px) and BOOST (about 64 px) bottom-right, at least 56 px each; a pause button top-right. The pause card gains Resume, **Drop test** (the X action, which also resumes) and a **Motion stunts** on/off toggle (remembered in localStorage).
- **Readouts as buttons (every device):** a tap or click on the wind readout cycles the wind like V (it always shows, "Off" with no wind); a tap on the air-mode label switches mode like M.
- **Motion stunts:** `devicemotion` `rotationRate` (deg/s), remapped for `screen.orientation.angle`, only while the car is airborne. A twist about the axis out of the screen of at least 220 deg/s rolls in the direction of the twist; a flick of the landscape screen's top edge of at least 260 deg/s is a front flip away from the player and a back flip toward them; the winning axis must be 1.5 times the others; a shake of at least 450 deg/s total with no clear axis is a front flip; 600 ms cooldown after a trigger (`web/src/ui/motion-gesture.ts`). iOS asks for permission when Play is pressed. A hint, "Twist to roll, flick to flip" (or "Swipe to roll or flip" with motion off), shows in the message line on the first jumps, at most 3 times per session.
- **Swipe fallback:** a swipe of at least 40 CSS px that starts on the right half of the screen, not on a button: left / right barrel roll, up front flip, down back flip. It uses the same request path as A S D F, so the planner still decides. It works whether or not motion is on.
- **Phone layout (landscape):** the desktop arrangement scaled down. Top row: hearts and saves, one message line (a toast replaces the hint), wind chip, pause. Bottom-centre: a slim speed and air panel (about 220 px wide) with the air-mode label as its top line, the nozzle map and g-meter flanking it. Edge pieces respect `env(safe-area-inset-*)` (`viewport-fit=cover`). Held upright, a full-screen "Rotate your phone" card shows and the game waits. On Play the game asks for full screen and a wake lock, best effort.
- **Gamepad** (standard mapping, polled once per frame; the key card lists these while one is connected; no vibration):

| Input | Action |
|---|---|
| Right / left trigger | gas / brake and reverse |
| Left stick X (dead zone 0.15) | steer |
| A (0) / B (1) | hop / boost |
| LB (4) / RB (5) | barrel roll left / right |
| Y (3) / X (2) | front flip / back flip |
| Start (9) | pause |

Buttons are edge-detected: a held button requests once.


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
- **Observations:** length 43; names unique; hash stable; values inside the spec bounds on random states.
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
- [x] `nl-wasm` exposes `Game`; the web app renders the car and ground and drives with the keyboard at 60 fps.
- [x] Chase camera, the fixed-step loop and interpolation work. Golden trace matches on wasm.

**M3 — Thrusters, tank, modes, events**
- [x] 10 nozzles per 7.4 with valve lag; the tank per 7.5; modes per 7.6; Hop/Boost per 7.8; events per 7.9.
- [x] All sign, tank, hop and event tests from section 12 pass.
- [x] Web: flames, nozzle map, air gauge, toasts, Manual controller with keys 1–0, Chaos button.

**M4 — PD baseline and eval harness**
- [x] `Pd` controller with allocation; a controller toggle in the web app.
- [x] Rust-side eval runner for Off/PD on Stages A and B (no Python needed), writing results JSON.
- [x] PD saves visibly more than Off on Stage A (numbers recorded in `docs/EXPERIMENTS.md`).

**M5 — Python env and first policy**
- [x] `nl-py` `BatchedSim` and `NineLivesVecEnv`; all Python tests pass.
- [x] PPO trains on Stage A with the curriculum; the export and parity tests pass; `models/policy.json` is committed.
- [x] `python -m ninelives.eval` produces results for Off/PD/AI on Stage A.
- [x] Up to 8 evenly spaced checkpoints are exported as policy JSON for the training viewer (§9.12).

**M6 — AI in the browser**
- [x] `load_policy`, with the hash check, and controller AI work in the game; drop tests in the browser match Python eval qualitatively.
- [x] The training viewer (§11.6) replays the exported checkpoints with a crowd of cars on Stage A drops.

**M7 — Hop, course, driver bot, full curriculum**
- [x] Course loader with all piece types and the `gauntlet.json`; the procedural generator; the driver bot.
- [x] The policy is trained and re-exported: `models/policy.json` is flight-r5 (the flight-r4 policy retrained on the widened draws), trained on Stages A, D, E and F (docs/EXPERIMENTS.md, DECISIONS 2026-11-09).
- Note: Stages B and C are not trained, on purpose: B almost never upsets the car and C is a hop, so PD flies them (the Hybrid hands over only past 120° of tilt, DECISIONS 2026-10-27).
- [x] Eval on all stages and both modes: Stages A–E, Off / PD / Hybrid, Realistic and Arcade, 1000 episodes each (`results/eval_*`). With the shipped r5 policy these seed 0 to 999 files show the Hybrid level with or slightly ahead of PD in every cell (Stage A Limited air 83.0% against 82.1%). On the fresh blocks of `results/flight-r5_2026-11/` PD is about one point ahead on Stage A and E Unlimited air, and the Hybrid is far ahead on Stage F; see the README.
- [x] The Gauntlet was playable start to finish with checkpoints (historical: cut from the game on 2026-11-03, DECISIONS; course and sim code stay).
- [x] An obstacle arena course (ramps and blocks, plain colours) that the training viewer runs.

**M8 — Polish and write-up**
- [x] Title screen, reduced motion, the performance overlay, static deploy (CI builds the Docker image and smoke-tests it with `deploy/smoke.sh`).
- [x] The hover-hack experiment (9.11): both runs, numbers and a height chart (`results/hover_hack.json`, `docs/images/hover-hack.svg`). Frame traces instead of video clips; see docs/EXPERIMENTS.md.
- [x] README: pitch, screenshots (no GIF), how it works, results charts, the hover hack, limits, how to run, credits and sources.

**M9 — Optional**
- [ ] Anatomy figure (car teardown: nozzles, tank, valves) for the landing page.
- [ ] Pressure falloff and the mass update in v2 physics.
- [ ] Gamepad support.

---

## 14. Risks and mitigations
| Risk | Mitigation |
|---|---|
| Training stops (laptop sleeps, restarts or is closed) | Resumable training chunks (`--resume`, `--max-minutes`); checkpoints kept in `runs/` on the training machine (ADR-003) |
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
- A shared leaderboard.
- Comparing RL with model-predictive control.

## 16. References
- Tesla Roadster thruster reveal, 2026-10-15: [TechCrunch](https://techcrunch.com/2026/10/15/tesla-reveals-spacex-powered-roadster-that-flies-and-falls-with-style/), [Glitchwire](https://glitchwire.com/news/the-tesla-roadster-hovered-flipped-and-landed-the-air-tank-weighed-260-kilos/)
- Rapier `DynamicRayCastVehicleController`: https://docs.rs/rapier3d/latest/rapier3d/control/struct.DynamicRayCastVehicleController.html
- PyO3: https://docs.rs/pyo3/latest/pyo3/
- Stable-Baselines3: https://pypi.org/project/stable-baselines3/
- Reward shaping walkthrough this project's style follows: Hari, "Training an AI to land a rocket with reward shaping"
