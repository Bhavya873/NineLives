# Nine Lives: technical overview

How the project is specified and how it is built. `SPEC.md` is the binding specification
and the place to look for every number; this document explains the design and points at
the code. Paths are relative to the repository root.

## 1. What the system does

A car has ten cold-gas thrusters. A controller fires them so that a car thrown into the air
(off a ramp, a kick plate or a drop) turns upright and lands on its wheels. Three controllers
exist: a hand-written PD controller, a neural-network policy trained with PPO, and a Hybrid
that gives the nozzles to the policy only during an upset. The same Rust simulation runs in
three places:

| Consumer | Binding | Purpose |
|---|---|---|
| Rust tests and the eval runner | direct | correctness, baselines, fixed-seed evaluation |
| Python training (`python/ninelives`) | PyO3 module `ninelives_sim` (`crates/nl-py`) | PPO on batched simulations |
| The browser game (`web/`) | wasm-bindgen `Game` (`crates/nl-wasm`) | play, the training viewer |

The design rule that holds this together: physics, thruster forces, the air tank, events,
observations and rewards exist only in `crates/nl-sim`. Python and TypeScript call into it and
never re-implement any of it.

## 2. Workspace layout

| Crate or folder | Contents |
|---|---|
| `crates/nl-sim` | world, vehicle, thrusters, tank, modes, courses, events, observation, reward, scenarios, episodes, driver bot |
| `crates/nl-control` | the `Controller` trait, Off, Manual, PD, thruster allocation, AI, Hybrid, the eval runner |
| `crates/nl-policy` | policy JSON loader and the MLP forward pass |
| `crates/nl-py` | `BatchedSim` for training, eval helpers |
| `crates/nl-wasm` | `Game` (one player car) and `Crowd` (the training viewer) |
| `python/ninelives` | Gymnasium/SB3 adapter, training, curriculum, eval, export, charts |
| `web/` | Vite + TypeScript + three.js game and training viewer |
| `configs/` | TOML for vehicle, modes, controllers, reward and scenarios; YAML for PPO runs |
| `courses/` | course JSON (playground, Gauntlet, the viewer's obstacle arena) |

## 3. Conventions

- SI units everywhere. Angles are radians in code and degrees only in config comments and UI.
- World frame: right-handed, +Y up, gravity (0, −9.81, 0), the same as three.js.
- Body frame: +X forward, +Y up, +Z right, origin at the body origin, which is the configured
  centre of mass (§5.1 covers the training offset).
- Rotation signs follow the right-hand rule: roll about +X raises the left side, yaw about +Y
  turns the nose left, pitch about +Z raises the nose.
- Physics step `dt = 1/120 s`; the controller runs every second step (60 Hz) and its throttles
  are held in between (`crates/nl-sim/src/conventions.rs`).
- Nozzle order `L_FL L_FR L_RL L_RR S_FL S_RL S_FR S_RR B_L B_R`; wheel order `FL FR RL RR`.
- `f32` in the simulation, the policy and wasm.

## 4. Determinism

Training, evaluation and the browser must agree bit for bit, so:

- Rapier is built with `enhanced-determinism`.
- All randomness is `ChaCha8Rng` seeded from an episode or world seed. Each purpose uses its own
  stream of that seed (randomization, sensor noise, course generation, driver bot, Stage E knocks,
  stage choice, wind), so adding a draw in one place never shifts another.
- Transcendental functions that feed decisions (`acos` for tilt, `exp` for the valve lag) use
  `libm`, whose results do not depend on the platform.
- No iteration over hash maps in simulation logic.
- A golden trace (2000 scripted steps, `crates/nl-sim/src/golden_trace.rs`) is hashed natively
  and in Node through wasm; both must match `crates/nl-sim/tests/golden/flat_drive.json`. The
  trace runs on frozen copies of the configs in `crates/nl-sim/tests/golden/`, so tuning the
  live configs does not change the hash; only a change to the simulation code does.
- `World::snapshot` and `World::restore` serialize the whole world with serde and postcard, under
  a format version (`SNAPSHOT_FORMAT_VERSION`). Controllers keep their own state outside the
  world and are not part of a snapshot.

## 5. Simulation (`crates/nl-sim`)

### 5.1 Vehicle body

The chassis is one dynamic rigid body: an 1800 kg box (half-extents 2.1 × 0.45 × 1.0 m) with
chamfered bottom edges so the nose rides up curved walls, plus a thin roof box so a roof contact
can be told apart from a body contact. Mass and inertia are set explicitly (about
(850, 3250, 2900) kg·m² for roll, yaw and pitch) instead of being derived from the colliders,
which have zero density (`build_chassis_body` in `crates/nl-sim/src/car.rs`).

`chassis.center_of_mass_offset_m` moves the centre of mass away from the body origin. It is 0 in
the game and in evaluation; training randomizes it by up to ±0.1 m per axis.

### 5.2 Wheels and driving

The wheels are Rapier's `DynamicRayCastVehicleController`: four suspension rays at
(±1.3, 0, ±0.85) m, radius 0.34 m, rest length 0.3 m and 0.2 m of travel, with spring and damping
given per kg of car mass. A wheel counts as touching only if its ray hits and the car is upright
enough, because the rays point down in the body frame and would hit things above an inverted car.
The wheel update is skipped while the car is upside down.

The drive model (`Car::engine_and_brake` and `engine_force_limit_n` in `car.rs`, `[drive]` in
`configs/vehicle.toml`):

- All-wheel drive. Each wheel's engine force is the throttle times
  `min(max_engine_force_per_wheel_n, max_engine_power_w / (4 · |v|))`: 7.2 kN per wheel at low
  speed, then constant power (800 kW) above about 28 m/s, so acceleration falls with speed as in
  a real car. The engine stops pushing at 103 m/s (371 km/h), reached in about 16 s; 0–100 km/h
  takes 2.0 s (research R2a). Boost fades out below 110 m/s: with Arcade's double thrust and
  unlimited air the car tops out at 108.7 m/s (391 km/h).
- Pulling back brakes while rolling forward and reverses below 0.5 m/s (reverse is capped at
  8 m/s). Braking is an impulse per wheel per step, about 1.1 g in total.
- With no pedal pressed, a small brake impulse (about 0.04 g) stands in for rolling resistance and
  engine braking.
- Tyre grip is Rapier's `friction_slip` (1.9, raised from 1.3 for the 2 s launch: a full-lock
  circle at 20 m/s peaks at about 1.57 g, and lateral grip at 100 m/s is about 1.67 g). Rapier's raycast wheels do not read collider friction, so the ground's
  friction coefficient only matters when the body itself scrapes.
- Steering turns the front wheels only, at most 3 rad/s, within a limit that shrinks from 0.5 rad
  when stopped to 0.25 rad at 50 m/s.

Air drag acts along each body axis, `F_i = −0.5 · ρ · CdA_i · |v_air| · v_air,i`, with `v_air`
the velocity through the (possibly windy) air in the body frame and CdA = 0.6, 8.0 and 5.5 m² for
front, top and side. It acts at a centre of pressure behind and above the centre of mass, so a
side wind turns the nose into the wind and leans the car downwind.

The chassis has Rapier continuous collision detection (`chassis.ccd_enabled`, on): at 100 m/s the
car moves 0.83 m per physics step, more than the roof box is thick. The wheels are raycasts, so
their contact does not depend on CCD; `tests/roadster_drive.rs` checks that all four rays hold at
100 m/s over floor seams and a ramp's kinks with CCD off and on, and that CCD stops a car at a wall.
`cargo run --release -p nl-sim --example drive_target` measures the roadster targets (0–100 km/h,
top speed, steering and lateral grip at 50–100 m/s, wheel stability at 100 m/s), and the driver bot
holds speed and line up to 55.6 m/s (`tests/driver_bot.rs`).

`cargo run --release -p nl-sim --example drive_feel` prints the 0–100 km/h time, top speed (also
with Boost held, per mode), braking distance and full-lock turn behaviour, for tuning these values.

### 5.3 Thrusters

Each nozzle has a body-frame position, a unit direction (the push on the car, opposite to the
exhaust) and a maximum force in Realistic mode:

| Nozzles | Direction | Max force | Main role |
|---|---|---|---|
| `L_*` at (±1.6, −0.3, ±0.85) | +Y | 12 kN each | lift, roll, pitch |
| `S_*` at (±1.4, +0.5, ±1.0) | ±Z, inwards | 4 kN each | roll, yaw, sideways |
| `B_*` at (−2.1, 0, ±0.4) | +X | 8 kN each | boost, yaw |

The 6 × 10 matrix `B` of full-throttle force and torque (`τ = r × F`) has rank 6, but the
achievable forces and torques are limited by one-sided thrust (no nozzle pushes down or
backwards), throttle bounds of 0 to 1, valve lag and the air left. Rank 6 does not mean any
combined demand, such as strong lift while braking a roll, can be met.

Each valve opens with a first-order lag (`Valves::update` in `crates/nl-sim/src/thrusters.rs`):

    u_eff ← u_eff + (u_cmd − u_eff) · (1 − exp(−dt / τ)),  τ = 0.03 s

`apply_nozzle_forces` then adds `u_eff · F_max · thrust_scale · tank_gate` along each nozzle's
direction at its position, both rotated into the world frame, with `add_force_at_point`.

**Thruster interlock** (`World::thrusters_locked`, `[thrusters.interlock]`): while the car is
steady on the ground (tilt ≤ 25°, centre of mass ≤ 1 m up, |ω| ≤ 2.5 rad/s, |v_y| ≤ 3 m/s) the
controller's throttles are replaced by zeros. Hop and Boost still fire. It exists because trained
policies learned to idle thrust while parked. No save is ever cut, because a falling, tilted or
spinning car is not steady.

### 5.4 Throttle pipeline and step order

`World::step_car` runs these steps once per physics step:

1. Driver input, then the Hop and Boost update (`HopBoost::update`).
2. Feed-forward throttles. Hop puts `hop_ff` on the four lift nozzles for `hop_s`. Boost puts
   `boost_ff` on both boost nozzles while held, scaled by `boost_speed_share`, which fades it to
   zero between 50 and 60 m/s of forward speed.
3. The controller's held throttles, or zeros if the interlock holds.
4. `u_cmd = clamp(u_ff + u_ctrl, 0, 1)` (`combine_throttles`).
5. Valve lag, drag, nozzle forces (scaled by mode and tank gate) and the wheels
   (`Car::prepare_physics_step`).
6. The Rapier step.
7. The tank pays for this step's thrust, then refills if allowed.
8. Course triggers (checkpoints, finish, kick plates), then events.

During a Hop the controller cannot lower lift below `hop_ff`, but it can add the remaining
throttle on any lift nozzle and use the other six freely.

### 5.5 Air tank and modes

Air use is `ṁ = F / v_e` with exhaust speed `v_e = 600 m/s`, so full lift (48 kN) uses 80 kg/s
(`crates/nl-sim/src/tank.rs`). Thrust fades linearly to zero over the last 2% of the tank instead
of cutting off (`AirTank::thrust_gate`). The tank refills at 20 kg/s while at least three wheels
touch and no air has been used for 0.5 s; this rate is a gameplay concession.

| | Realistic | Arcade |
|---|---|---|
| Thrust scale | 1.0 | 2.0 |
| Tank | 260 kg | unlimited (air used is still counted) |
| Hop | 0.75 lift for 0.40 s, 1.5 s cooldown | 0.75 lift for 0.35 s, 0.8 s cooldown |

### 5.6 Events

The world queues events and the caller drains them (`crates/nl-sim/src/events.rs`):

- **Airborne** starts when no wheel touches and ends when any wheel touches.
- **Upset** starts after 0.3 s airborne or above 60° of tilt.
- **Save**: during an upset, four wheels down, tilt < 15° and |ω| < 1.5 rad/s, held for 0.5 s. It
  carries the touchdown spin and fall speed, and a landing quality: the heading error against the
  last take-off from the wheels, and the share of forward speed kept 0.5 s after touchdown.
- **Peak g**: the raw per-tick seat g (`GLoad::raw_g`, before the 0.1 s filter) is maxed over a stunt
  window from `AirborneStart` to 0.5 s after the touchdown that ends the airtime, resets at the next
  take-off, a respawn and a placement, and goes out in `Save.peak_g` and `HudField.PeakGLoad`. Every
  wasm event is 5 floats (`FLOATS_PER_EVENT`); Save's fifth is `peak_g`, the others pad 0.
- **Hard landing**: a suspension compression of 0.95 or more within 0.2 s of a touchdown.
- **Crash**: a body or roof contact faster than 6 m/s along the contact normal, any roof contact
  above 120° of tilt, more than 100° of tilt for 2 s, or leaving the course bounds. A contact with
  the flat floor is judged on its true normal (straight up): Rapier's box-box manifold sometimes
  reports a tipped normal for the car's bevelled edge, which turned a car sliding at 50 m/s into a
  "46 m/s impact" (244 of the R2c baseline's 1,295 crashes).
  The solver gets the same fix: `nl-sim/src/floor_hook.rs` puts the true vertical normal back on a
  tipped chassis-versus-floor contact (top face only), so a 1.6 m/s rear-edge touchdown no longer
  becomes a 7 m/s bounce and a -8.8 rad/s yaw.
- **Scrape** (wasm `EventKind` 15): a body contact that is underbody (0.05 m inside the lower face),
  with its normal within 30° of body up, at most 4 m/s along it, with the tilt under 25°. Numbers in
  `configs/approach.toml`. It never counts toward a crash and reports once per scrape.
- **TooSlowFallback** (`EventKind` 16): a clean entry onto a piece below its `v_complete_min_m_s`.
  `traversal.rs` keeps a `PieceTraversal` per pass (entry tick, entry speed, `Approach`: heading,
  lateral offset, pitch, roll, wheels down, `clean`, `reason`) judged against `approach.toml`; it is
  read by tests, the HUD hint and the survival metric and never makes the car immune.

In the game a crash freezes the wreck and the car respawns at the last checkpoint after 1.5 s. In
training a crash ends the episode.

### 5.7 Courses

Courses are JSON (schema version 1, `crates/nl-sim/src/course.rs`). They are checked strictly
(field ranges, at most 200 pieces, unknown fields refused, errors that name the piece and the
field) and built into static colliders (`course_build.rs`). Ramps, kickers and corkscrews are
single curved trimeshes with Rapier's internal-edge fix, because separate convex segments stopped
cars on their seams. A half loop is a half circle of road the car rides up and over; its road is
slick (friction 0.1) because the loop's 6 g bottoms out the suspension and the body scrapes. Checkpoints, the finish and kick plates are trigger boxes tested against the
path of the centre of mass since the last step, so a fast car cannot step over a 0.5 m line. A
generator (`course_gen.rs`, `configs/pieces.json`) builds random straight training courses from
the same pieces, including "flip plates": named presets of linear and angular impulse that throw
the car into front flips, back flips, barrel rolls and flat spins.

Piece geometry follows `docs/contracts/ramp-curvature.md`: curved pieces have an entry radius of
at least 110 m (a design load of 3.9 g at 200 km/h against the bump stop's measured 4.2 g), so a
ramp is `R · angle` long, and no two neighbouring segments meet at more than 1.2°. Jump lips stay
at 8° or less. A wall with a ramp is a table top (ramp, deck, ramp down), a gap can hold a catch
pit (sloped floor, exit slope), and bumps are as long as their height needs for 4.6° faces. The
generator lays each run-out past the farthest landing at 55.6 m/s (`flight_distance_m`) and
`pieces.json` publishes `v_complete_min_m_s` per piece. The suspension has a bump stop
(`[wheels] bump_stop_*`): past 0.14 m of compression an extra spring of 300 (m/s²)/m per kg joins.

### 5.8 Wind and disturbances

Wind is a steady mean in a horizontal direction plus Ornstein–Uhlenbeck gusts on each horizontal
axis, stepped exactly (`g ← a·g + σ·√(1 − a²)·N(0, 1)`, `a = e^(−dt/τ)`) from its own random
stream (`wind.rs`). The levels are Off, Breeze (5 m/s) and Storm (15 m/s). A Chaos knock is a
random linear impulse of up to 8 kN·s plus an angular impulse of up to 3 kN·m·s; training
Stage E uses it.

## 6. Controllers (`crates/nl-control`)

Every controller implements:

```rust
pub trait Controller {
    fn reset(&mut self, seed: u64);
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT]; // throttles in [0, 1]
}
```

`ControlCtx` holds the driver input, the `CarState` (gravity direction, angular and linear
velocity in the body frame, height, wheel contacts, thrust scale, tank gate) and the observation
built from it. Runners call `act` when `World::is_control_step()` is true and pass the result to
`World::set_controller_throttles`.

### 6.1 PD

PD (`pd.rs`, gains in `configs/controllers.toml`) builds a wanted body-frame wrench and lets the
allocation turn it into throttles. It stays silent while the car drives on its wheels.

- **Attitude.** With `a` the body axis that should point up (body +Y, tipped by any pitch wish)
  and `up_des = −g_dir_body`, the error is `e = a × up_des`. The wanted angular acceleration is
  `α = kp·e − kd·(ω − ω_wish)`, capped at 25 rad/s² for the passenger, and the torque is `I·α`,
  with `I` the body-axis inertia. The gains follow the tilt: one pair while roughly upright,
  a stiffer pair once tumbling, blended linearly in between (`tumble_blend_start_deg` to
  `tumble_blend_end_deg`).
  Near upside down the cross product has no direction, so PD picks a roll. A car already spinning
  towards upright fast enough to make it the long way round is allowed to continue.
- **Vertical braking.** When falling faster than 4 m/s and lower than `max(3 m, 1.3·d_stop + 1 m)`,
  with `d_stop = (v² − v_t²) / (2a)`, `v_t` the target descent speed and `a` the net braking
  acceleration available, it asks for `F_y = m(g + kv(v_down − v_t))`.
- **Sideways damping.** `F_z = −m · kv_lat · v_z` before touchdown.
- **On the ground, tipped over.** Roll torque times a ground gain, plus a side-booster push, to
  roll back onto the wheels.
- **Air control.** Above 3 m and upright, the driver's steer asks for a yaw rate and the throttle
  for a nose-down pitch.

### 6.2 Allocation

`Allocator` (`allocation.rs`) solves the bounded least-squares problem

    minimise ‖W(B·u − w)‖² + λ‖u‖²  subject to 0 ≤ u ≤ 1

by projected gradient descent with a fixed iteration count and a step of `1/L`, where `L` comes
from a Gershgorin bound on the largest eigenvalue. It is warm-started from the previous control
step's answer. Each row of `B` is scaled to 1 at its strongest nozzle and then weighted, with
torque rows weighing more than force rows. The target is divided by the mode's thrust scale but
not by the tank gate: dividing by the fading gate would drive every nozzle to full throttle as the
tank runs dry.

`docs/control-upgrade-audit.md` lists what this allocation leaves out (valve lag, Hop and Boost
headroom, the tank gate, the interlock, the centre of mass under randomization). The legacy PD
baseline that any change is judged against is in `results/control_upgrade/`.

With `[allocation] model = "actuator_aware"` PD instead plans with the actuator predictor
(`crates/nl-sim/src/actuator_model.rs`). `World::actuator_state` copies the valves, tank and
Hop/Boost timers, and `ActuatorState::predict` steps those copies through the next two physics
steps with the same functions as `World::step_car`. It returns the mean push about the true
centre of mass, the Hop/Boost part of it, the saturated nozzles, the air used and the interlock.
The allocation aims the interval's mean push at PD's wrench on top of Hop and Boost, and keeps
each nozzle within the headroom they leave. `Pd::last_allocation` reports what PD asked for,
what is predicted and the difference. Under latency (domain randomization) an episode passes no
predictor, because the valves then get an older command, so PD falls back to the legacy
allocation. Training hands PD the same state (`TrainingEnv::actuator_state`), so a Hybrid or
residual policy learns over the PD the game ships.

PD's numbers and `lag_compensation` are tuned together by `python -m ninelives.tune_pd`, a
CMA-ES search over how hard PD pushes (never when it acts) on tuning seeds 2,000,000+, scored
on success with a cost for air used over the bumps strip; candidates are then checked on the
validation seeds (`docs/EXPERIMENTS.md`, 2026-10-27).

### 6.3 AI

`Ai` (`ai.rs`) runs the exported policy on the observation and maps each action `a ∈ [−1, 1]` to
the throttle `(a + 1) / 2`. The forward pass (`crates/nl-policy`) normalizes the observation with
the saved running mean and variance, clips it, and runs the MLP (two hidden layers of 128 with
tanh). It is deterministic: it uses the mean action.

### 6.4 Hybrid

`HybridGate` (`hybrid.rs`) gives the nozzles to the AI when the tilt passes the upset angle (60°)
and back to PD once the car has settled the way a save requires (four wheels down, tilt < 15°,
spin < 1.5 rad/s, held for 0.5 s). It latches, so a bounce never flips control back and forth.
Being airborne alone does not hand over: PD lands ordinary jumps well, and handing those to the
AI cost Stage D success. The game, both eval runners and training share this gate, so the shipped
controller is judged under the rule it runs.

The gate also applies the policy envelope (`envelope.rs`, `PolicyEnvelope::is_outside`): when the
body forward speed is above `[hybrid] policy_max_speed_m_s` (55 m/s unless raised) or a saturating
observation value is on its clamp bound, `update` reports PD in control, `policy_out_of_envelope()`
is true, and `Game::hud()[HudField.PolicyOutOfEnvelope]` (27) is 1. The latch keeps running by its
normal rules meanwhile. The wasm `Game` computes the flag with the same function, so native, wasm,
the crowd and the eval agree. Raise the limit only after logging a Hybrid eval at that speed.

## 7. Reinforcement learning

### 7.1 Batched environment

`BatchedSim(n_envs, config_json, seed)` (`crates/nl-py`) owns N independent episodes and steps
them in parallel with rayon. Actions arrive as an N × 10 array in [−1, 1]. An environment that
ends returns its terminal observation in `info` and its reset observation in `obs`. Each finished
episode is reported as a JSON row with the same fields as the eval results. Python reads no
physics files: the TOML texts are passed in as one JSON config.

### 7.2 Observation

43 values, defined once in `crates/nl-sim/src/obs.rs` (`OBS_SPEC`), each clamped to a stated
range:

| Index | Content |
|---|---|
| 0–2 | world-down in the body frame |
| 3–5 | angular velocity, body frame |
| 6–8 | linear velocity, body frame |
| 9 | height above the ground (ray from the centre of mass, capped at 20 m) |
| 10–17 | wheel contacts and suspension compressions |
| 18–20 | air fraction, thrust scale, unlimited air |
| 21–23 | Hop active, Hop time left, Boost active |
| 24–25 | driver throttle and steer |
| 26–30 | terrain height 2–10 m ahead along the heading |
| 31–40 | the previous commanded throttles |
| 41–42 | the heading error: sin and cos of the nose's yaw from the take-off heading, both 0 with no heading to keep (`landing::heading_error_sin_cos`) |

The schema hash is FNV-1a 64 over every field's name and range. The trainer, the exporter and the
browser all check it before trusting a policy.

### 7.3 Episodes and stages

`Episode` (`episode.rs`) is the one place where success, failure and the time limit are decided.
It applies the per-episode randomization, a latency queue (the valves receive the throttles of
0–2 control steps ago), sensor noise on what controllers see, the driver bot's input and the
scheduled knocks.

| Stage | Start | Success |
|---|---|---|
| A, drop | 1.5–8 m up, tilt up to 180°, spin up to 6 rad/s, moving | four wheels down for 1 s with quiet throttles |
| B, tip-over | lying on its side | back on four wheels without touching the roof |
| C, hop | driving at 5–25 m/s, Hop over an optional car row | a save after the hop, no row touched |
| D, course | a generated course with the driver bot | reach the finish |
| E, chaos | A, C or D plus 1–3 knocks | the base stage's success after every knock |
| F, flight | 5 m before a strong flip plate at 10–28 m/s | land and hold, heading within 30° |

Stages C, D and F also fail an episode whose landing turned more than 30° away from the take-off
heading.

### 7.4 Reward

Per control step, with weights in `configs/reward.toml`: tilt and spin costs; an air cost (the
tank share used, or the summed throttle with unlimited air); a cost for airtime beyond 0.5 s; a
smoothness cost on throttle changes; a passenger g-load cost above 3 g; and a cost for every step
spent unsettled after the first touchdown. Events add a save bonus scaled by landing quality, −10
for a crash and −1 per hard landing; success adds +5 plus the share of the tank left. There is no
survival bonus. Paying for being alive teaches a policy to hover, which the
`configs/train/hover-*.yaml` experiment demonstrates.

### 7.5 Domain randomization

Drawn per episode from its own seed: mass and inertia ×0.9–1.1, centre-of-mass offset ±0.1 m,
per-nozzle force ×0.85–1.15, valve τ 0.02–0.06 s, latency 0–2 control steps, observation noise,
ground friction 0.6–1.2, wind up to Storm, thrust scale 1–2, and unlimited air with probability
0.4 (otherwise a 150–350 kg tank). Eval leaves it off unless `--dr` is given.

### 7.6 Training

Training uses PPO from Stable-Baselines3 (`python/ninelives/train.py`):
- 64 environments, 512 steps each per update, batch 4096, 10 epochs;
- learning rate 3e-4 decaying linearly, γ 0.99, GAE λ 0.95;
- two hidden layers of 128 with tanh;
- observations and rewards normalized by `VecNormalize`;
- throttle means that start at the closed end (`initial_action_bias: -1`).

A curriculum raises Stage A's tilt and spin limits in five levels and unlocks later stages when
the current one passes an eval bar at a 1M-step checkpoint. Unlocked stages keep being sampled, so
earlier skills are not forgotten. With `controller: hybrid`, PD flies inside the simulator, and
each AI stretch between the gate's hand-over and its release is one episode for the policy. Every
run is logged in `docs/EXPERIMENTS.md`.

### 7.7 Export

`python -m ninelives.export` writes `models/policy.json`: the observation schema hash, the
normalizer's mean, variance and clip, every layer's weights (row-major, out × in) and activation,
and the action mapping. A parity test runs 1000 random observations through SB3 and through the
Rust forward pass and requires a maximum difference below 1e-4. Up to eight evenly spaced
checkpoints are exported for the training viewer.

## 8. Evaluation

`cargo run --release -p nl-control --example eval -- --controller pd --stage A --mode realistic
--episodes 1000 --seed 0` and `python -m ninelives.eval` run the same Rust episode code and give
identical rows. Results go to `results/eval_<controller>_<stage>_<mode>.json`, one row per
episode: success, saves, crash cause, air used, peak tilt, touchdown spin and fall speed, hard
landings, heading error, speed kept, settle time and air left. Each file also carries
`physics_hash`, `controller_config_hash`, `policy_hash` and `git_sha` (`nl_sim::provenance`, FNV-1a),
so a result can be matched to the settings it ran on; `--out <dir>` picks the folder. Seeds are split into blocks:
0–999 (the spent M7 tests), 1,000,000–2,999,999 for validation and tuning, and
3,000,000–3,000,999 for the one-time final test. Controllers are compared on identical seeds.

## 9. Browser game

`Game` (`crates/nl-wasm/src/lib.rs`) wraps one `World`, the controllers and the loaded policy.
`tick(input)` advances exactly one physics step from a packed 4-byte input (throttle, steer and
16 button bits). On control steps it runs the controller, OR-ing together the buttons pressed
since the last one so a quick tap is not lost. The page reads poses, nozzle throttles, HUD values
and drained events as flat `Float32Array`s, whose layouts are checked at start-up
(`checkWasmLayout` in `web/src/main.ts`).

The loop (`web/src/game-loop.ts`) collects real time and spends it in fixed 1/120 s ticks, at most
8 per frame. The renderer blends the last two physics poses, so motion is smooth at any frame
rate. Rendering (`web/src/render`), input (`web/src/input`) and the HUD (`web/src/hud`) are
separate modules. The look is a physics simulator's: a checkered floor, a gradient sky and fog.
The game flies PD alone (`?controller=hybrid` loads the policy and flies the Hybrid), shows the keys at all times,
pauses on Esc, and gives the player nine lives, one lost per crash.

The training viewer (`web/viewer.html`) builds one wasm `Crowd` per exported checkpoint and flies
them all at once on the same episodes: one labelled row of translucent cars per checkpoint,
plus a table of landings and crashes. Every car is flown by the Hybrid (`build_controller("hybrid", ...)`,
the hand-over from `controllers.toml`) with the checkpoint as its AI. `Crowd::for_stage` covers Stage A
drops (flat and the obstacle arena), Stage D and F on a generated course (`episode::course_for_stage`, one
course per crowd, drawn from `course_shapes()`) and the Gauntlet; a stage picker (or `?arena=`) chooses.
`cargo run --release -p nl-wasm --example viewer_rates` flies the same crowds natively and records the
landing rates (`results/baseline_2026-10/viewer_landing_rates.json`).

## 10. Testing

| Layer | What is tested |
|---|---|
| Rust (`cargo test --workspace`) | sign tests for every nozzle force and torque, valve step response, tank flow, driving (acceleration, power limit, Boost limit, grip, braking), events, courses, observation layout and hash, reward terms, episode rules, PD and Hybrid on flip plates and courses, determinism, the golden trace, snapshot round trips |
| Python (`pytest python/tests`) | the environment against Gymnasium's checker, config overrides, the curriculum, export parity |
| Web (`npm run test:wasm`, Playwright) | the wasm golden trace, the Game API, HUD text and toasts; in a real browser: driving, PD by default and the `?controller=hybrid` switch, drop tests, pause, hearts, the training viewer in both arenas |

The lints are clippy with `-D warnings`, ruff, ESLint and Prettier.
