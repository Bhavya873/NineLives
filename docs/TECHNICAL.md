# Nine Lives: technical overview

How the project is specified and how it is built. `SPEC.md` is the binding specification
and the place to look for every rule; this document explains the design, gives the current
numbers and points at the code. Paths are relative to the repository root. Numbers quoted
from `configs/` are the values on `main` when this was written; if they differ, the config
file wins.

## 0. The project in words

**One line.** Nine Lives is a browser driving game in which a controller fires a car's ten
cold-gas thrusters, so that whatever throws the car into the air, it turns upright and lands
on its wheels.

**One paragraph.** A car drives around an arena of ramps, kickers, corkscrews and flip
plates. When it leaves the ground tumbling, ten small air thrusters on the body (four pushing
up, four pushing sideways, two pushing forward) fire on their own to turn it upright, slow
its fall and set it down pointing the way it was going. One deterministic Rust physics
simulation, built on the Rapier engine, serves everything: unit tests, a fixed-seed
evaluation runner, reinforcement-learning training in Python (PPO), and the game itself,
which runs the same simulation compiled to WebAssembly under a three.js renderer. Two kinds
of controller compete on identical seeds: a hand-built PD attitude controller with a
constrained thruster allocator, and a neural-network policy trained with PPO, which flies
inside a "Hybrid" that hands it the nozzles only during an upset.

**A longer description.** The project is part game and part controls experiment. The car is
an 1800 kg rigid body on four raycast wheels, with a power-limited all-wheel-drive model, air
drag, and wind with gusts. Its ten thrusters have valves that lag like real solenoid valves,
and their air comes from a finite tank, so every newton of thrust costs mass at the exhaust
speed (`ṁ = F / v_e`). Every rule a controller is judged by (what counts as an upset, a save,
a hard landing or a crash; when an episode succeeds) lives in one Rust crate, so training,
evaluation and the browser cannot disagree. Determinism is enforced end to end: seeded
ChaCha8 streams per purpose, Rapier's `enhanced-determinism`, `libm` for any maths that feeds
a decision, and a golden trace whose hash must match natively and in wasm. Six stages form
both the curriculum and the benchmark: drops, tip-overs, hops over parked cars, procedurally
generated courses with a scripted driver, random knocks, and flights off strong flip plates.
The current result is that the hand-built controller wins. After its model was fixed and its
gains tuned with CMA-ES, it lands 94.8% of validation episodes across all stages and both
modes, ahead of every trained Hybrid. So the game flies PD by default and keeps the AI behind
a URL switch.

**Key facts.**

| | |
|---|---|
| Languages | Rust (simulation, controllers, bindings), Python (training), TypeScript (game) |
| Physics | Rapier 3D 0.36.1 with `enhanced-determinism`; physics at 120 Hz, control at 60 Hz |
| Car | 1800 kg; 10 thrusters (48 kN lift, 16 kN side, 16 kN boost in Realistic); 260 kg air tank |
| Observation and action | 41 values in, 10 throttles out |
| Policy | MLP 41 → 128 → 128 → 10 with tanh, trained with Stable-Baselines3 PPO |
| Training speed | about 28k env-steps/s with 64 environments on an 8-core M1 Pro (Stage A) |
| Browser cost | about 0.017 ms per physics tick in release wasm (budget 1.5 ms) |
| Best controller | tuned PD: 94.84% mean success plain, 88.74% under domain randomization |
| Code size | about 36k lines across 5 crates, 1 Python package and the web app |

**Status.** Milestones M0 to M6 are complete (SPEC §13). M7 (driving stages, courses, a
policy trained on every stage) is complete except for its last box: no all-stage policy has
yet beaten the tuned PD, so `models/policy.json` is still the Stage A policy `stage-a-004`.
M8 (polish, deploy, README) has not started.

## 1. What the system does

A car has ten cold-gas thrusters. A controller fires them so that a car thrown into the air
(off a ramp, a kick plate or a drop) turns upright and lands on its wheels. Five controllers
exist: Off, Manual (debug keys), a hand-written PD controller, a neural-network policy (AI)
trained with PPO, and a Hybrid that gives the nozzles to the policy only during an upset. The
game flies PD alone; `?controller=hybrid` in the URL loads the policy and flies the Hybrid.

The same Rust simulation runs in three places:

| Consumer | Binding | Purpose |
|---|---|---|
| Rust tests and the eval runner | direct | correctness, baselines, fixed-seed evaluation |
| Python training (`python/ninelives`) | PyO3 module `ninelives_sim` (`crates/nl-py`) | PPO on batched simulations |
| The browser game (`web/`) | wasm-bindgen `Game` and `Crowd` (`crates/nl-wasm`) | play, the training viewer |

The design rule that holds this together: physics, thruster forces, the air tank, events,
observations and rewards exist only in `crates/nl-sim`. Python and TypeScript call into it and
never re-implement any of it.

## 2. Architecture and data flow

```
configs/*.toml, configs/pieces.json, courses/*.json
        │  (passed in as text; Rust embeds the defaults with include_str!)
        ▼
  nl-sim ── World, Episode, TrainingEnv, events, observation, reward
        │
  nl-control ── Controller trait: Off, Manual, Pd (+ Allocator), Ai, Hybrid, ResidualPd
        │            └─ nl-policy ── policy JSON loader and MLP forward pass
        │
   ┌────┴────────────────┬───────────────────────────┐
   ▼                     ▼                           ▼
 eval example         nl-py (PyO3)                nl-wasm (wasm-bindgen)
 results/*.json       BatchedSim, run_eval        Game, Crowd, eval_episodes
                         │                           │
                      python/ninelives            web/ (Vite, three.js)
                      train  → runs/<id>/         index.html: the game
                      export → models/policy.json viewer.html: the training viewer
                             → web/public/checkpoints/
```

The policy makes a round trip. Python trains it against the Rust physics, `export.py` writes
it as JSON, and Rust (`nl-policy`) runs its forward pass again in eval and in the browser. A
parity test keeps the two forward passes equal.

### 2.1 Workspace layout

| Crate or folder | Contents |
|---|---|
| `crates/nl-sim` | world, vehicle, thrusters, tank, modes, courses, events, observation, reward, scenarios, episodes, driver bot |
| `crates/nl-control` | the `Controller` trait, Off, Manual, PD, thruster allocation, AI, Hybrid, residual PD, the eval runner |
| `crates/nl-policy` | policy JSON loader and the MLP forward pass |
| `crates/nl-py` | `BatchedSim` for training, eval helpers, Hybrid-gated and residual training slots |
| `crates/nl-wasm` | `Game` (one player car) and `Crowd` (the training viewer) |
| `python/ninelives` | Gymnasium/SB3 adapter, training, curriculum, eval, compare, export, PD tuning |
| `web/` | Vite + TypeScript + three.js game and training viewer |
| `configs/` | TOML for vehicle, modes, controllers, reward and scenarios; JSON course pieces; YAML for PPO runs |
| `courses/` | course JSON: `playground.json` (the game), `gauntlet.json`, `arena.json` (the viewer), `bumps-strip.json` (an air-use test) |
| `results/` | committed eval JSON and CSV, and the control-upgrade result folders |
| `models/policy.json` | the shipped policy (about 500 kB) |
| `docs/` | `DECISIONS.md`, `EXPERIMENTS.md`, `PLAN.md`, `TRAINING.md`, ADRs, the control audit |

### 2.2 Module map of `nl-sim`

| Concern | Files |
|---|---|
| Constants and frames | `conventions.rs` (dt, rates, counts, names), `input.rs` (`InputFrame`), `hash.rs` (FNV-1a) |
| Configuration | `vehicle_config.rs`, `modes.rs`, `scenarios.rs`, `reward.rs`, `randomization.rs`, `wind.rs` |
| The world | `world.rs` (`World`: the step, snapshots, accessors), `car.rs` (chassis, wheels, drive, drag), `contacts.rs` |
| Thrust | `thrusters.rs` (valves, nozzle forces), `tank.rs` (`AirTank`), `hop_boost.rs`, `hop_profile.rs`, `actuator_model.rs` |
| What controllers see | `car_state.rs` (`CarState`, `CarModel`, `Touchdown`), `obs.rs` (`OBS_SPEC`, `observe`) |
| Rules | `events.rs` (`EventTracker`, `SimEvent`, `CrashCause`), `landing.rs` (take-off, heading error, speed kept), `g_load.rs` |
| Courses | `course.rs` (schema and checks), `course_build.rs` (colliders), `course_mesh.rs` (lofted trimeshes), `course_triggers.rs`, `course_gen.rs` (generator), `checkpoint.rs` |
| Episodes | `episode.rs` (`Episode`, `EpisodeResult`), `stage_start.rs`, `driving_stages.rs` (C, D, E), `flight_stage.rs` (F), `driver_bot.rs`, `chaos.rs` |
| Training | `training.rs` (`TrainingEnv`, stage mixes, auto-reset) |
| Determinism | `golden_trace.rs` |

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
- Library code returns `Result` with `thiserror` errors. Clippy denies `unwrap` and `expect`
  outside tests, and `unsafe` is forbidden across the workspace.

## 4. Determinism

Training, evaluation and the browser must agree bit for bit, so:

- Rapier is built with `enhanced-determinism`.
- All randomness is `ChaCha8Rng` seeded from an episode or world seed. Each purpose uses its own
  stream of that seed (randomization, sensor noise, course generation, driver bot, Stage E knocks,
  stage choice, wind), so adding a draw in one place never shifts another.
- Transcendental functions that feed decisions (`acos` for tilt, `exp` for the valve lag, `cos`
  for the Hybrid gate, `tanh` in the policy) use `libm`, whose results do not depend on the
  platform.
- No iteration over hash maps in simulation logic.
- A golden trace (2000 scripted steps, `crates/nl-sim/src/golden_trace.rs`) is hashed natively
  and in Node through wasm; both must match `crates/nl-sim/tests/golden/flat_drive.json`
  (currently `e664ebfb5e9aa416`). The trace runs on frozen copies of the configs in
  `crates/nl-sim/tests/golden/`, so tuning the live configs does not change the hash; only a
  change to the simulation code does.
- `World::snapshot` and `World::restore` serialize the whole world with serde and postcard, under
  a format version (`SNAPSHOT_FORMAT_VERSION`, now 17). Controllers keep their own state outside
  the world and are not part of a snapshot.
- A test pins same-seed training runs as bit-identical. A NaN or infinity in an observation,
  reward or weight stops training without overwriting `latest/`.

## 5. Simulation (`crates/nl-sim`)

### 5.1 Vehicle body

The chassis is one dynamic rigid body: an 1800 kg box (half-extents 2.1 × 0.45 × 1.0 m). Its
bottom front and rear edges are chamfered (0.6 m long, 0.3 m high: about 27° approach and
departure angles) so the nose rides up curved walls. A thin roof box (half-extents
1.2 × 0.05 × 0.8 m) lets a roof contact be told apart from a body contact. Mass and inertia
are set explicitly, (850, 3250, 2900) kg·m² for roll, yaw and pitch, instead of being derived
from the colliders, which have zero density (`build_chassis_body` in
`crates/nl-sim/src/car.rs`).

`chassis.center_of_mass_offset_m` moves the centre of mass away from the body origin. It is 0 in
the game and in evaluation; training randomizes it by up to ±0.1 m per axis.

`CarModel` (`car_state.rs`, `World::car_model`) is what PD knows about the car: its mass, the
body-axis inertia, and each nozzle's force and torque at full throttle about the true centre
of mass. (Rapier's `principal_inertia()` returns yaw and pitch swapped, so the model reads the
body-axis values directly.)

### 5.2 Wheels and driving

The wheels are Rapier's `DynamicRayCastVehicleController`: four suspension rays at
(±1.3, 0, ±0.85) m, radius 0.34 m, rest length 0.3 m and 0.2 m of travel. Spring and damping
are given per kg of car mass (stiffness 50, compression damping 3.5, relaxation damping 5.0,
maximum force 40 kN), so each wheel sinks about 0.05 m at rest and the centre of mass sits
about 0.6 m up. A wheel counts as touching only if its ray hits and the car is upright enough,
because the rays point down in the body frame and would hit things above an inverted car.
"On its wheels" means at least three touch (`ON_WHEELS_MIN_WHEELS`). The wheel update is
skipped while the car is upside down (tilt past `INVERTED_TILT_DEG`, 150°).

The drive model (`Car::engine_and_brake` and `engine_force_limit_n` in `car.rs`, `[drive]` in
`configs/vehicle.toml`):

- All-wheel drive. Each wheel's engine force is the throttle times
  `min(max_engine_force_per_wheel_n, max_engine_power_w / (4 · |v|))`: 3 kN per wheel at low
  speed, then constant power (220 kW, about 300 hp) above about 18 m/s, so acceleration falls
  with speed as in a real car (0–100 km/h in about 5 s). The engine stops pushing at 55 m/s
  (198 km/h).
- Pulling back brakes while rolling forward and reverses below 0.5 m/s (reverse is capped at
  8 m/s). Braking is an impulse of 40 N·s per wheel per step, about 1.1 g in total.
- With no pedal pressed, a small brake impulse (1.5 N·s per wheel per step, about 0.04 g)
  stands in for rolling resistance and engine braking.
- Tyre grip is Rapier's `friction_slip` (1.3, sporty road tyres on dry asphalt: a full-lock
  circle peaks at about 1.13–1.2 g). Rapier's raycast wheels do not read collider friction, so
  the ground's friction coefficient only matters when the body itself scrapes.
- Steering turns the front wheels only, at most 3 rad/s, within a limit that shrinks from 0.5 rad
  when stopped to 0.25 rad at 50 m/s.

Air drag acts along each body axis, `F_i = −0.5 · ρ · CdA_i · |v_air| · v_air,i`, with
`ρ = 1.2 kg/m³`, `v_air` the velocity through the (possibly windy) air in the body frame, and
CdA = 0.6, 8.0 and 5.5 m² for front, top and side. It acts at a centre of pressure 0.3 m behind
and 0.1 m above the centre of mass, so a side wind turns the nose into the wind and leans the
car downwind.

`cargo run --release -p nl-sim --example drive_feel` prints the 0–100 km/h time, top speed (also
with Boost held, per mode), braking distance and full-lock turn behaviour, for tuning these values.

### 5.3 Thrusters

Each nozzle has a body-frame position, a unit direction (the push on the car, opposite to the
exhaust) and a maximum force in Realistic mode (`[[thrusters.nozzles]]` in `vehicle.toml`):

| Index | Nozzles | Position (m) | Direction | Max force | Main role |
|---|---|---|---|---|---|
| 0–3 | `L_FL L_FR L_RL L_RR` | (±1.6, −0.3, ±0.85) | +Y | 12 kN each | lift, roll, pitch |
| 4–7 | `S_FL S_RL S_FR S_RR` | (±1.4, +0.5, ±1.0) | ±Z, inwards | 4 kN each | roll, yaw, sideways |
| 8–9 | `B_L B_R` | (−2.1, 0, ±0.4) | +X | 8 kN each | boost, yaw |

Left nozzles sit at −Z and the left side nozzles push towards +Z; the right ones do the
reverse. The side nozzles sit 0.5 m above the centre of mass, so a side push also rolls the
car; the lift nozzles sit 0.3 m below it.

The 6 × 10 matrix `B` of full-throttle force and torque (`τ = r × F`,
`nozzle_force_and_torque_at_full_throttle`) has rank 6. The achievable forces and torques are
still limited by one-sided thrust (no nozzle pushes down or backwards), throttle bounds of 0 to
1, valve lag and the air left. Rank 6 does not mean any combined demand, such as strong lift
while braking a roll, can be met.

Each valve opens with a first-order lag (`Valves::update` in `crates/nl-sim/src/thrusters.rs`):

    u_eff ← u_eff + (u_cmd − u_eff) · (1 − exp(−dt / τ)),  τ = 0.03 s

So within one 1/60 s control interval a valve shows only about a third of a command change.
`apply_nozzle_forces` then adds `u_eff · F_max · thrust_scale · tank_gate` along each nozzle's
direction at its position, both rotated into the world frame, with `add_force_at_point`.

**Thruster interlock** (`World::thrusters_locked`, `[thrusters.interlock]`): while the car is
steady on the ground (tilt ≤ 25°, centre of mass ≤ 1 m up, |ω| ≤ 2.5 rad/s, |v_y| ≤ 3 m/s) the
controller's throttles are replaced by zeros. Hop and Boost still fire. It exists because trained
policies learned to idle thrust while parked. No save is ever cut, because a falling, tilted or
spinning car is not steady.

### 5.4 Throttle pipeline and step order

`World::step(input)` runs `step_car` once per physics step:

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

A runner drives the world like this (the game's version; eval and training build the context
with `ControlCtx::for_episode`, which adds the episode's sensor noise and latency):

```rust
if world.is_control_step() {                     // every second physics step
    let ctx = ControlCtx::for_world(&world, input); // CarState, Observation, ActuatorState
    world.set_controller_throttles(controller.act(&ctx));
}
world.step(input);                               // one 1/120 s step
for event in world.drain_events() { /* toasts, rewards, results */ }
```

### 5.5 Air tank and modes

Air use is `ṁ = F / v_e` with exhaust speed `v_e = 600 m/s`, so full lift (48 kN) uses 80 kg/s
(`crates/nl-sim/src/tank.rs`) and empties a full 260 kg tank in about 3.25 s. Thrust fades
linearly to zero over the last 2% of the tank instead of cutting off (`AirTank::thrust_gate`).
The tank refills at 20 kg/s (empty to full in about 13 s) while at least three wheels touch and
no air has been used for 0.5 s; this rate is a gameplay concession.

| `configs/modes.toml` | Realistic | Arcade |
|---|---|---|
| Thrust scale | 1.0 | 2.0 |
| Tank | 260 kg | unlimited (air used is still counted) |
| Hop | 0.75 lift for 0.40 s, 1.5 s cooldown | 0.75 lift for 0.35 s, 0.8 s cooldown |
| A Hop needs | 30 kg of air | (unlimited) |
| Boost | full throttle on both boost nozzles | the same, at twice the force |

`HopBoost` reports why a Hop is refused (`HopBlockedReason`: `NotOnGround`, `Cooldown`,
`NoAir`), and the HUD shows it.

### 5.6 Events

The world queues events and the caller drains them (`crates/nl-sim/src/events.rs`, thresholds
in `vehicle.toml [events]`):

- **Airborne** starts when no wheel touches and ends when any wheel touches.
- **Upset** starts after 0.3 s airborne or above 60° of tilt.
- **Save**: during an upset, four wheels down, tilt < 15° and |ω| < 1.5 rad/s, held for 0.5 s. It
  carries the touchdown spin and fall speed, and a landing quality. The quality has the heading
  error against the last take-off from the wheels, and the share of forward speed kept 0.5 s
  after touchdown (measured only after a take-off faster than 3 m/s).
- **Hard landing**: a suspension compression of 0.95 or more within 0.2 s of a touchdown. It is
  an event and a reward cost, not a crash.
- **Crash** (`CrashCause`): a body or roof contact faster than 6 m/s along the contact normal
  (`HardImpact`), any roof contact above 120° of tilt (`UpsideDown`), more than 100° of tilt
  for 2 s (`Stuck`), or falling below −20 m (`OutOfBounds`).
- **Checkpoint reached**, **Finished**, **Kicked** (a kick plate fired) and **Respawn** come
  from the course.

The passenger's g-load is measured at the driver's seat (−0.2, 0.15, −0.35 m from the centre
of mass) and smoothed over 0.1 s (`g_load.rs`, `World::g_load`). The reward charges for it
above 3 g and the game darkens the screen.

In the game a crash freezes the wreck and the car respawns at the last checkpoint after 1.5 s. In
training a crash ends the episode.

### 5.7 Courses

Courses are JSON (schema version 1, `crates/nl-sim/src/course.rs`). They are checked strictly:
every number finite, sizes 0.1–500 m, angles in [1, 60)°, at most 200 pieces, bounds within
±5 km, unknown fields refused, and errors that name the piece and the field. Then they are
built into static colliders (`course_build.rs`).

Piece types: `ramp`, `kicker` (a one-sided ramp that rolls the car), `corkscrew` (a twisted
ramp), `gap`, `car_row` (parked 4.5 × 1.9 m boxes; `hop_hint` tells the driver bot to Hop),
`halfpipe`, `bumps`, `wall` (optionally with a 10 m ramp), `landing_pad`, `kick_plate` (a
trigger that applies a linear and angular impulse), `checkpoint` and `finish`. Ramps, kickers
and corkscrews are single curved trimeshes lofted between cross-sections (`course_mesh.rs`)
with Rapier's internal-edge fix, because separate convex segments stopped cars on their seams.
Checkpoints, the finish and kick plates are trigger boxes tested against the path of the centre
of mass since the last step, so a fast car cannot step over a 0.5 m line. Each shape carries a
`Surface` (floor, ramp, obstacle or a marking) that the renderer colours by, and obstacle
colliders are tagged (`OBSTACLE_USER_DATA`) so touching a car row can be detected.

The generator (`course_gen.rs`, `configs/pieces.json`) builds random straight training courses
of 4–8 pieces along +X on a 20 m wide track. Each piece is followed by a flat run-out (the
landing zone) and a checkpoint. Its ranges stay within what the driver bot with PD can finish:
ramps up to 18°, kickers 3.5–4.5 m wide, gaps cleared at 10 m/s. "Flip plates" are kick plates
whose impulses come from named presets (front flip, back flip, a barrel roll each way, roof
toss, flat spin), scaled by a strength. Generated courses draw mild plates (strength 1.0–1.1,
which the thrusters catch early). Strength 3 ("strong") turns the car past 120° even with the
thrusters on; strong plates appear in the playground, the flip-plate tests and Stage F.

The game opens on `courses/playground.json`, an open arena with a row of each obstacle type.
`courses/gauntlet.json` is a start-to-finish course (kicker, gap, a car row to Hop, halfpipe,
bumps and kick plates, a big ramp, a wall with a ramp, the finish) kept for the sim and eval.
`courses/arena.json` is the training viewer's obstacle arena.

### 5.8 Wind and disturbances

Wind is a steady mean in a horizontal direction plus Ornstein–Uhlenbeck gusts on each horizontal
axis, stepped exactly (`g ← a·g + σ·√(1 − a²)·N(0, 1)`, `a = e^(−dt/τ)`) from its own random
stream (`wind.rs`):

| Level | Mean | Gust σ | Gust τ |
|---|---|---|---|
| Off | 0 | 0 | — |
| Breeze | 5 m/s | 1.5 m/s | 2.0 s |
| Storm | 15 m/s | 5 m/s | 1.5 s |

A Chaos knock (`chaos.rs`) is a random linear impulse of up to 8 kN·s plus an angular impulse
of up to 3 kN·m·s, in random directions; training Stage E uses it.

## 6. Controllers (`crates/nl-control`)

Every controller implements:

```rust
pub trait Controller {
    fn reset(&mut self, seed: u64);
    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT]; // throttles in [0, 1]
}
```

`ControlCtx` holds the driver `input`, the `state` (`CarState`: gravity direction, angular and
linear velocity in the body frame, height, wheel contacts, thrust scale, tank gate), the
`observation` built from it, and `actuators`, the thrusters' own state for the actuator-aware
allocation (§6.2; `None` under latency). Runners call `act` when `World::is_control_step()` is
true and pass the result to `World::set_controller_throttles`.

Controllers are picked by name through one list (`controller_kind.rs`). `ControllerKind` is a
plain enum over `Off`, `Pd`, `Ai`, `Manual` and `Hybrid`, built by `build_controller` from
`ControllerChoices`: PD for this car, the optional shared `Arc<Policy>`, and the Hybrid gate
settings. The game's T key cycles `off → pd → ai → hybrid`. Manual (keys 1–0 fire nozzles 0–9)
is reachable only through `set_controller("manual")`.

### 6.1 PD

PD (`pd.rs`, gains in `configs/controllers.toml [pd]`) builds a wanted body-frame wrench and
lets the allocation turn it into throttles. It stays silent while the car drives on its wheels
(two or more wheels down, tilt ≤ 15°, spin ≤ 1.5 rad/s).

- **Attitude.** With `a` the body axis that should point up (body +Y, tipped by any pitch wish)
  and `up_des = −g_dir_body`, the error is `e = a × up_des`. The wanted angular acceleration is
  `α = kp·e − kd·(ω − ω_wish)`, capped at 25 rad/s² for the passenger, and the torque is `I·α`,
  with `I` the body-axis inertia. The gains follow the tilt: kp 6.625 and kd 5.083 while roughly
  upright, kp 39.46 and kd 4.532 once tumbling, blended linearly between 57.9° and 83.0° of
  tilt. Near upside down the cross product has no direction, so PD picks a roll. A car already
  spinning towards upright fast enough to make it the long way round is allowed to continue.
- **Vertical braking.** It acts when airborne, roughly upright (up alignment above 0.7),
  falling faster than 4 m/s and lower than `max(3 m, 1.3·d_stop + 1 m)`. Here
  `d_stop = (v² − v_t²) / (2a)`, `v_t` is the target descent speed (0.25 m/s) and `a` the
  braking acceleration from 80% of full lift. It asks for `F_y = m(g + kv(v_down − v_t))`
  with kv 6.0.
- **Sideways damping.** `F_z = −m · kv_lat · v_z` before touchdown, with kv_lat 0.93 /s.
- **On the ground, tipped over.** Below 1.2 m, not upright and barely falling: roll torque times
  a ground gain (3.5), plus a side-booster push of 0.33 of the car's weight along whichever side
  axis points up, to roll back onto the wheels.
- **Air control.** Above 3 m and upright, the driver's steer asks for a yaw rate (up to
  1.5 rad/s) and the throttle for a nose-down pitch (up to 0.35 rad). Training and eval give
  PD no driver input, so this acts only in the game (a known gap, DECISIONS 2026-10-21).

### 6.2 Allocation

`Allocator` (`allocation.rs`, `configs/controllers.toml [allocation]`) solves the bounded
least-squares problem

    minimise ‖W(B·u − w)‖² + λ‖u‖²  subject to 0 ≤ u ≤ 1

by projected gradient descent with a fixed 30 iterations and a step of `1/L`, where `L` comes
from a Gershgorin bound on the largest eigenvalue. It is warm-started from the previous control
step's answer. Each row of `B` is scaled to 1 at its strongest nozzle and then weighted: torque
rows 3, force rows 1, `λ = 0.001`. The target is divided by the mode's thrust scale but not by
the tank gate: dividing by the fading gate would drive every nozzle to full throttle as the
tank runs dry.

`[allocation] model` selects one of two models:

- **`actuator_aware`** (the default). PD plans with the actuator predictor
  (`crates/nl-sim/src/actuator_model.rs`). `World::actuator_state` copies the valves, tank and
  Hop/Boost timers, and `ActuatorState::predict` steps those copies through the next two
  physics steps with the same functions as `World::step_car`. It returns the mean push about
  the true centre of mass, the Hop/Boost part of it, the saturated nozzles, the air used and
  the interlock. The allocation aims the interval's mean push at PD's wrench on top of Hop and
  Boost, keeps each nozzle within the headroom they leave, and makes up `lag_compensation`
  (0.357) of the valve lag within one interval. `Pd::last_allocation` reports what PD asked
  for, what is predicted and the difference. Under latency (domain randomization) an episode
  passes no predictor, because the valves then get an older command, and PD falls back to the
  legacy allocation. Training hands PD the same state (`TrainingEnv::actuator_state`), so a
  Hybrid or residual policy learns over the PD the game ships.
- **`legacy`** plans as if `B·u` happened at once.

`docs/control-upgrade-audit.md` lists what the allocation still leaves out.
`results/control_upgrade/2026-10-26-legacy-pd/` holds the old PD baseline.

`python -m ninelives.tune_pd` tunes PD's numbers and `lag_compensation` together. It is a
CMA-ES search (the `cma` package) over 12 multipliers on how hard PD pushes, never on when it
acts: every "when" threshold, the comfort cap and the air-control feel stay frozen. The
2026-10-27 search ran 30 generations of 12 candidates, at 200 episodes per cell over stages
A–F and both modes, on tuning seeds 2,000,000+. It scored success, with a cost for air used
over the bumps strip. The top candidates were then checked on validation seeds (§8).

### 6.3 AI

`Ai` (`ai.rs`) runs the exported policy on the observation and maps each action `a ∈ [−1, 1]` to
the throttle `(a + 1) / 2`. The forward pass (`crates/nl-policy/src/policy.rs`) normalizes the
observation with the saved running mean and variance, clips it to the saved bound (±10), and
runs the MLP (two hidden layers of 128 with tanh, then a linear output clipped to ±1). It is
deterministic: it uses the mean action. A policy is refused (`PolicyError`) if its format,
version, dimensions or observation schema hash do not match.

### 6.4 Hybrid

`HybridGate` (`hybrid.rs`) gives the nozzles to the AI when the tilt passes the upset angle. That
is 60°, from `vehicle.toml [events]`, unless `[hybrid] enter_tilt_deg` in `controllers.toml`
sets another. It hands back to PD once the car has settled the way a save requires (four wheels
down, tilt < 15°, spin < 1.5 rad/s, held for 0.5 s). It latches, so a bounce never flips control
back and forth. PD acts every step even while the AI flies, so its warm start is current at
hand-back. Being airborne alone does not hand over: PD lands ordinary jumps well, and handing
those to the AI cut Stage D success from 99% to 12%. The game, both eval runners and training
share the gate through `HybridSwitch`, so the shipped controller is judged under the rule it
runs.

### 6.5 Residual PD

`ResidualPd` (`residual.rs`) is the dropped alternative to the Hybrid. PD flies and the policy
adds a scaled nudge, `u = clamp(u_pd + s · a, 0, 1)`, so a zero action is exactly PD. The nudge
can be divided by the mode's thrust scale, and an optional eleventh action opens a gate
`(a + 1) / 2` that scales every nudge. A NaN action falls back to PD. It trains through `nl-py`
(`configs/train/residual*.yaml`), but no residual policy ships: the best checkpoint of the
100M-step run was its first (EXPERIMENTS rounds 5–6).

## 7. Reinforcement learning

### 7.1 Batched environment

`BatchedSim(n_envs, config_json, seed)` (`crates/nl-py/src/batched_sim.rs`) owns N independent
`TrainingEnv`s and steps them in parallel with rayon (Rapier's own `parallel` feature stays off).
Actions arrive as an N × 10 array in [−1, 1] (N × 11 for gated residual). An environment that
ends returns its terminal observation in `info` and its reset observation in `obs` (SPEC §9.1
auto-reset). Each finished episode is reported as a JSON row with the same fields as the eval
results, and the reward is also returned split into named terms for logging. Python reads no
physics files: `sim_config.py` reads the TOML texts and passes them in as one JSON config.

Its other methods are `reset`, `reseed`, `level_count`, `set_stage_and_level` and
`set_stage_mix` (the curriculum's per-stage weights and levels). The module also exports
`obs_spec`, `obs_schema_hash`, `nozzle_names`, `physics_dt_s`, `policy_format`,
`reward_term_names` and `run_eval`. On the Python side, `env.py` wraps `BatchedSim` as an SB3
`VecEnv` (`NineLivesVecEnv`) and a single-env Gymnasium `NineLivesEnv` for Gymnasium's checker.

Each slot (`env_slot.rs`) runs one of three control set-ups:

- **Plain:** the action is the throttles.
- **Residual:** the slot runs PD and adds the action (§6.5).
- **Hybrid-gated** (`hybrid_pd.rs`, `controller: hybrid` in the train YAML): PD flies, hidden
  from the policy, until the game's gate hands over. Each AI stretch, from that hand-over to
  the gate's release, a crash or the time limit, is one episode for the policy, so it trains
  on exactly the flights it gets in the game. Sim episodes PD flies alone are skipped. Each Save
  the AI-flown episode earns under hidden PD before the next hand-over is credited to the
  release step; on A, B and F, so are its success and air-left bonuses.

### 7.2 Observation

41 values, defined once in `crates/nl-sim/src/obs.rs` (`OBS_SPEC`), each clamped to a stated
range:

| Index | Content |
|---|---|
| 0–2 | world-down in the body frame |
| 3–5 | angular velocity, body frame |
| 6–8 | linear velocity, body frame |
| 9 | height above the ground (ray from the centre of mass, capped at 20 m) |
| 10–17 | wheel contacts and suspension compressions |
| 18–20 | air fraction, thrust scale (÷ 3), unlimited air |
| 21–23 | Hop active, Hop time left, Boost active |
| 24–25 | driver throttle and steer |
| 26–30 | terrain height 2, 4, 6, 8 and 10 m ahead along the heading |
| 31–40 | the previous commanded throttles |

The schema hash is FNV-1a 64 over every field's name and range (`16d2f5faa7964d05` for v1).
The trainer, the exporter and the browser all check it before trusting a policy.

### 7.3 Episodes and stages

`Episode` (`episode.rs`) is the one place where success, failure and the time limit are decided.
It applies the per-episode randomization, a latency queue (the valves receive the throttles of
0–2 control steps ago), sensor noise on what controllers see, the driver bot's input and the
scheduled knocks. `stage_start.rs` places the car for each stage, and `start_episode`
(`scenarios.rs`) is the single entry point.

| Stage | Start (`configs/scenarios.toml`) | Success | Limit |
|---|---|---|---|
| A, drop | 1.5–8 m up, tilt up to 180°, spin up to 6 rad/s, 0–15 m/s sideways, −5 to +3 m/s vertical | four wheels down for 1 s with quiet throttles | 8 s |
| B, tip-over | lying on its side (60–110° of roll) just above the ground | as A: four wheels down for 1 s with quiet throttles, without touching the roof | 8 s |
| C, hop | driving at 5–25 m/s on a flat track; Hop over a row of 1–3 parked cars (half the time) | a save after the hop, no car touched | 15 s |
| D, course | a generated course (§5.7) with the driver bot at 10–30 m/s | reach the finish | 45 s |
| E, chaos | A, C or D (equal odds) plus 1–3 knocks at 0.5–6 s | the base stage's success after every knock | 45 s |
| F, flight | 5 m before a strong flip plate (strength 2–3) at 10–28 m/s; the bot keeps driving | land and hold, heading within 30° | 10 s |

"Quiet throttles" means the controller's throttle sum, ignoring nozzles below 0.02, stays
below 0.05 for the last 0.5 s, so blowing air while parked is not a finished episode. Stages C,
D and F also fail an episode whose landing turned more than 30° away from the take-off heading.

The **driver bot** (`driver_bot.rs`, training and eval only) follows the course centreline by
pure pursuit (look-ahead 6 m + 0.3 s × speed, turns capped at 6 m/s² sideways). It holds a
target speed with a proportional throttle, adds an Ornstein–Uhlenbeck steering wobble and rare
steering jerks, centres the steering in the air, and presses Hop before a car row marked
`hop_hint` (with up to ±0.15 s of timing noise).

### 7.4 Reward

Per control step, with weights in `configs/reward.toml` (`reward_terms` in `reward.rs`):

| Term | Per-step value | Weight |
|---|---|---|
| tilt | `up · world_up − 1` (0 upright, −2 inverted) | 0.05 |
| spin | `−min(|ω|², 50)` | 0.002 |
| air, limited tank | `−air used this step / tank capacity` | 8.0 |
| air, unlimited | `−Σ throttles` | 0.008 |
| airborne | −1 per step airborne beyond 0.5 s | 0.02 |
| smoothness | `−|u_t − u_(t−1)|²` | 0.002 |
| g-force | `−(g − 3)²` above 3 g | 0.05 |
| unsettled | −1 per unsettled step after the first touchdown | 0 (off) |

Events add a save bonus of `2 + 3q`, with landing quality
`q = (1 − |ω|/4) · (1 − v_fall/6) · (1 − heading/45°) · speed kept` (each factor clamped to
0–1). A crash costs 10 and each hard landing 1. Success adds +5 plus the share of the tank left.
There is no survival bonus. Paying for being alive teaches a policy to hover, which the
`configs/train/hover-*.yaml` experiment demonstrates.

### 7.5 Domain randomization

Drawn per episode from its own seed (`randomization.rs`, `[randomization]`):
- mass and inertia ×0.9–1.1, centre-of-mass offset ±0.1 m;
- per-nozzle force ×0.85–1.15, valve τ 0.02–0.06 s, latency 0–2 control steps;
- observation noise: σ 0.01 on gravity direction, 0.05 rad/s on spin, 0.1 m/s on speed,
  0.05 m on height;
- ground friction 0.6–1.2, wind 0–1 × Storm in a random direction;
- thrust scale 1–2, and unlimited air with probability 0.4 (otherwise a 150–350 kg tank).

A per-mode run sets `randomize_mode: false` to keep its mode preset. Eval leaves randomization
off unless `--dr` is given.

### 7.6 Training

Training uses PPO from Stable-Baselines3 on CPU PyTorch (`python/ninelives/train.py`, settings
in `configs/train/base.yaml`):

- 64 environments, 512 steps each per update (32,768 samples), batch 4096, 10 epochs;
- learning rate 3e-4 decaying linearly to 0, γ 0.99, GAE λ 0.95, clip 0.2, no entropy bonus;
- two hidden layers of 128 with tanh, for both the policy and the value network;
- observations and rewards normalized by `VecNormalize` (observations clipped to ±10);
- throttle means that start at the closed end (`initial_action_bias: -1`, `log_std_init: 0`),
  so the deterministic policy idles at exactly 0 while half the samples still fire.

The curriculum (`curriculum.py`, `callbacks.py`) raises Stage A's tilt and spin limits in five
levels, from 30° with no spin to 180° at 6 rad/s. A level goes up when the deterministic eval
at a checkpoint passes 85%, 85%, 75% and then 65%. The next stage unlocks once the newest one
matches PD at its hardest level on the validation seeds, or after 15M steps without an unlock,
so later stages are never starved. Unlocked stages keep being sampled (by default A 0.25,
B 0.05, C 0.1, D 0.4, E 0.2), so earlier skills are not forgotten. Every 1M steps a checkpoint
is saved and scored on 200 fixed-seed episodes per stage, and kept as `best/` if it is the
best so far.

A run lives in `runs/<id>/` (`run_files.py`): `latest/` (always saved on stop, even on Ctrl-C),
`best/`, numbered checkpoints, `state.json` and TensorBoard logs. Flags include `--resume`,
`--init-from <run>/best` (warm start), `--keep-curriculum`, `--total-steps` and
`--max-minutes`. Checkpoints stay on the training machine and are never committed.

### 7.7 Export

`python -m ninelives.export` writes `models/policy.json` (format `ninelives-policy`, version 1):
- the observation schema hash and the dimensions;
- the normalizer's mean, variance and clip;
- every layer's weights (row-major, out × in) and activation;
- the action mapping (`(a+1)/2`, clipped to ±1);
- provenance (`trained.run_id`, steps, git SHA).

A parity test runs 1000 random observations through SB3 and through the Rust forward pass and
requires a maximum difference below 1e-4. `--viewer` exports up to eight evenly spaced
checkpoints, rounded to 4 significant digits, to `web/public/checkpoints/<run-id>/` with an
`index.json`.

The shipped policy is `stage-a-004`: 48M steps, Stage A only, git `34b3e91`.

### 7.8 What has been trained

`docs/EXPERIMENTS.md` logs every run. In short:

| When | Runs | What happened |
|---|---|---|
| 2026-10-17 | `stage-a-001`…`004` | Stage A alone. `stage-a-004` beat the PD of the time on Stage A (81% against 67%) and ships. |
| 2026-10-18/19 | `m7-full-001`…`004` | All stages A→E, 150M steps each. Reward hacks fixed (idling thrust, never settling); still lost to PD on D and E. |
| 2026-10-19/20 | ablations, `residual-100m` | Cost ablations and residual RL. Residual's best checkpoint was its first, so it was dropped. |
| 2026-10-21 | Hybrid, `flight-pilot` | The Hybrid gate met the M7 bar with `stage-a-004`; gated training and Stage F introduced. |
| 2026-10-26 | R1, R2 | Hybrid-gated retrains over the old PD; R2 reached 89.1% as a Hybrid on the new car. |
| 2026-10-27 | PD CMA-ES tune | PD from 90.77% to 94.84% plain, 83.80% to 88.74% randomized; adopted. |
| 2026-10-27 | R3, R3b, gate tilt | Hybrids over the tuned PD lose 10 of 12 cells; the AI lands Stage F turned (median 11–19° against PD's 1–2°). A later hand-over (90–150°) helps, but no setting wins every cell. |

## 8. Evaluation

`cargo run --release -p nl-control --example eval -- --controller pd --stage A --mode realistic
--episodes 1000 --seed 0` and `python -m ninelives.eval` run the same Rust episode code
(`nl_control::eval::run_episode`) and give identical rows. Results go to
`results/eval_<controller>_<stage>_<mode>.json` (and `.csv`), one row per episode: success,
saves, crash cause, air used, peak tilt, touchdown spin and fall speed, hard landings, heading
error, speed kept, settle time and air left. `python -m ninelives.compare` pairs two
controllers' files on the same seeds. It prints success with Wilson 95% intervals and judges
the pair against the M7 bar.

Seeds are split into blocks:
- 0–999: spent on the M7 verdict;
- 1,000,000–2,999,999: validation (1,500,000+) and tuning (2,000,000+);
- 3,000,000–3,000,999: the one-time final test, still untouched.

Controllers are compared on identical seeds, with at least 1000 episodes per cell for any
go/no-go call.

The current PD's success (tuned, `results/control_upgrade/2026-10-27-pd-cma-tune/`), on
validation seeds 1,500,000+ with 1000 episodes per cell:

| Stage | Realistic | Arcade | Realistic, randomized | Arcade, randomized |
|---|---|---|---|---|
| A, drop | 83.5% | 88.7% | 84.6% | 84.6% |
| B, tip-over | 99.5% | 99.4% | 99.6% | 99.6% |
| C, hop | 100.0% | 100.0% | 95.2% | 98.5% |
| D, course | 98.5% | 99.7% | 95.2% | 95.2% |
| E, chaos | 92.2% | 94.5% | 87.4% | 88.6% |
| F, flight | 89.5% | 92.6% | 68.2% | 68.2% |
| **Mean** | **94.84%** plain | | **88.74%** randomized | |

Under randomization the mode itself is drawn per episode, so the two randomized columns mostly
coincide. The bumps strip checks that plain driving costs almost no air: PD uses 6.22 kg over
it, against a limit of 8 kg. Many of Stage A's misses are cars let go exactly upside down with
no spin, where PD's attitude error has no preferred direction (a known limit, DECISIONS
2026-10-21).

## 9. Browser game

### 9.1 The wasm API

`Game` (`crates/nl-wasm/src/lib.rs`) wraps one `World`, the controllers and the loaded policy.
The page builds it from the three TOML texts and a seed:
`new Game(vehicleToml, modesToml, controllersToml, seed)`.

| Call | What it does |
|---|---|
| `tick(input)` | one physics step from a 4-byte `InputFrame` (§9.2) |
| `set_mode`, `set_wind_level`, `set_controller`, `cycle_controller` | switch mode, wind or controller |
| `load_policy(json)`, `has_policy()` | load and check `policy.json` |
| `load_course(json)`, `load_flat_ground()` | start a run on a course (returns the shapes to draw) or on the flat arena |
| `drop_test(seed)` | a Stage A start, to watch a save |
| `chassis_pose`, `wheel_poses` | 7 floats per pose: position, then quaternion x, y, z, w |
| `nozzle_throttles`, `nozzle_layout` | effective throttles and flame lengths; nozzle positions and directions |
| `hud()` | 23 floats indexed by `HudField`: speed, time, wheels, air, Hop state, mode, saves, crashes, checkpoints, finish, g-load, wind and more |
| `drain_events()` | flat floats per event: an `EventKind`, then up to three values |
| `snapshot`, `restore`, `set_checkpoint`, `is_crashed` | state and respawn |
| `build_hash`, `golden_trace_hash`, layout constants | start-up checks |

On control steps `tick` runs the controller, OR-ing together the buttons pressed since the
last one so a quick tap is not lost. The page checks the flat array layouts at start-up
(`checkWasmLayout` in `web/src/main.ts`). `Crowd` (`crowd.rs`) flies many episodes with one
policy for the training viewer and returns their poses, outcomes and a results JSON.
`eval_episodes` runs eval rows from JavaScript.

`npm run wasm` (`web/scripts/build-wasm.mjs`) builds `nl-wasm` for `wasm32-unknown-unknown` and
runs `wasm-bindgen` (pinned to 0.2.129, the version in `Cargo.lock`) into `web/src/wasm`. The
release build (`npm run wasm:release`) uses the `wasm-release` profile (`opt-level = "s"`, fat
LTO, one codegen unit) and then `wasm-opt` from `binaryen`.

### 9.2 Input and controls

`InputFrame` is four bytes: throttle as an `i8` (−127 brake or reverse to 127), steer as an `i8`
(positive is left, matching the yaw sign), and 16 button bits: Hop, Boost, Chaos and Reset,
then one bit per manual nozzle from bit 4. `web/src/input/keyboard.ts` builds it from the held
keys.

| Key | Action |
|---|---|
| W / ↑, S / ↓ | throttle; brake and reverse |
| A / ←, D / → | steer |
| Space | Hop |
| Shift | Boost |
| R | reset to the last checkpoint |
| X | drop test from a random start |
| M | switch Realistic and Arcade |
| V | cycle the wind: Off, Breeze, Storm |
| Esc | pause |
| F3 | performance overlay |
| 1–0 | fire nozzles 0–9 (Manual controller) |

### 9.3 Loop, rendering and HUD

The loop (`web/src/game-loop.ts`) collects real time and spends it in fixed 1/120 s ticks, at
most 8 per frame. The renderer blends the last two physics poses (`render/pose-blend.ts`), so
motion is smooth at any frame rate. Rendering (`web/src/render`), input (`web/src/input`) and
the HUD (`web/src/hud`) are separate modules, wired together in `main.ts`.

- **Scene** (`render/scene.ts`, `backdrop.ts`): a physics-lab look, with a blue-grey checkered
  floor, a gradient dome whose horizon colour matches the fog, and neutral light.
- **Car** (`car-shape.ts`, `car-mesh.ts`): a generic low sports car extruded from side
  outlines, with wheel arches, rims, mirrors and a front intake, and no brand shapes. Each
  thruster shows as a flush port on the body surface nearest its physics nozzle
  (`nozzle-ports.ts`), with an additive blue-white flame cone whose length follows its
  throttle (`flames.ts`).
- **Course** (`course-view.ts`): drawn from the shape list `load_course` returns, the same
  shapes the colliders were built from, so what you see is what you hit.
- **Camera** (`chase-camera.ts`): a smoothed chase camera that stays level with the world.
- **Effects**: a crash explosion with debris and camera shake (`explosion.ts`), wind streaks
  (`wind-streaks.ts`), and a g-force blackout vignette above 3 g (`hud/g-blackout.ts`).
- **HUD**:
  - speed and status text (`hud.ts`);
  - an air gauge, with ∞ in Arcade and a red flash below 15% (`air-gauge.ts`);
  - a top-down nozzle map whose dots glow with each throttle (`nozzle-map.ts`);
  - toasts: "Saved" or "Flip saved" with the air used, "Crashed" with the cause, and
    "Out of air" (`toasts.ts`);
  - a wind arrow (`wind-indicator.ts`);
  - nine pixel hearts and a game-over screen (`lives.ts`);
  - the F3 overlay (`perf-overlay.ts`).

The game opens in the playground in Realistic mode with PD. It shows the keys at all times,
pauses on Esc, and gives the player nine lives, one lost per crash.

### 9.4 Training viewer

The training viewer (`web/viewer.html`, `web/src/viewer/`) reads
`web/public/checkpoints/index.json` and builds one wasm `Crowd` per exported checkpoint. It
flies them all at once on the same Stage A drops: one labelled row of translucent cars per
checkpoint, on the flat arena or the obstacle arena, with a table of landings and crashes. It
shows how the policy improved over training.

## 10. Build, tooling and CI

| Area | Tools and versions |
|---|---|
| Rust | stable toolchain (edition 2024, minimum 1.85), rustfmt, clippy; `rapier3d` 0.36.1, `rand_chacha` 0.10, `postcard`, `serde`, `toml`, `thiserror` 2, `libm`, `rayon`, `pyo3` 0.30, `numpy` 0.30, `wasm-bindgen` 0.2.129 |
| Python | 3.11 or 3.12; `numpy`, `gymnasium` ≥ 1.2, `stable-baselines3` ≥ 2.9, `torch` (CPU), `pyyaml`, `tensorboard`; for development `maturin`, `pytest`, `ruff`, `cma` |
| Web | Node ≥ 22.18, Vite 8, TypeScript 6 (strict), three.js 0.186, ESLint 10, Prettier 3, Playwright 1.64, binaryen |

The native release profile uses thin LTO for training speed. CI (`.github/workflows/ci.yml`)
runs on every pull request and on pushes to `main`, as three jobs:

- **rust:** `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -D warnings`,
  `cargo test --workspace --locked`.
- **python:** install from `python/requirements-dev.lock`, `maturin develop --release`, `ruff
  check`, `ruff format --check`, `pytest`.
- **web:** read the `wasm-bindgen` version from `Cargo.lock` and install the matching CLI;
  `npm ci`, lint, build wasm, the Node wasm tests, `npm run build`, then Playwright on Chromium.

`graphify-out/graph.json` is a code map built locally by graphify from the tree-sitter AST. Query
it with `graphify explain`, `query` and `path` instead of grepping.

## 11. Testing

| Layer | What is tested |
|---|---|
| Rust (`cargo test --workspace`) | sign tests for every nozzle force and torque (on the ground and in flight), valve step response, tank flow, Hop and Boost, driving (acceleration, power limit, Boost limit, grip, braking), the interlock, events, courses and their triggers, the generator, the driver bot, observation layout and hash, reward terms, randomization signs, episode rules, the actuator predictor, the allocation, PD and Hybrid on flip plates, flip recovery, the Gauntlet and driving stages, determinism, the golden trace, snapshot round trips, respawn |
| Python (`pytest python/tests`) | the environment against Gymnasium's checker, config overrides, the curriculum, training smoke runs, Hybrid-gated and residual training, export parity, eval, compare and PD tuning |
| Web, Node (`npm run test:wasm`) | the wasm golden trace, the Game API, controllers, courses, the crowd, policy loading, the game loop, toasts, thruster visuals, the explosion and screen effects |
| Web, browser (Playwright) | driving, PD by default and the `?controller=hybrid` switch, drop tests, pause, hearts, HUD text, courses, the training viewer in both arenas, no console errors |

Tests read like examples (`firing_front_left_lift_raises_the_left_front_corner`) and follow arrange,
act, assert. Every new force, torque or frame transform gets its sign test first.

## 12. Known limits and open work

- **The AI does not yet beat PD.** Every trained Hybrid loses Stage F because the AI lands
  turned, and the observation has no heading error from take-off. The proposed next step is to
  add it (observation v2, which changes the schema hash and needs a retrain) and to train on
  plain episodes as well as randomized ones.
- **PD and a still, exactly inverted car.** With no spin and the car exactly upside down, PD's
  attitude error has no direction, so it only nudges the roll. Hybrid's AI handles this case.
- **No driver input in training.** Training and eval give PD no driver keys, so PD's air
  control acts only in the game.
- **Allocation gaps**, listed in `docs/control-upgrade-audit.md`: the predictor is skipped under
  latency, and PD does not know the centre-of-mass offset that randomization adds.
- **Not built yet (M8):** ghost runs, a title screen, a results tab, reduced motion, a static
  deploy, the hover-hack write-up with clips, and a README with charts. Later ideas (pressure
  falloff, mass that drops as air is used, gamepad support) are in SPEC §13 and §15.

## 13. Glossary

| Term | Meaning |
|---|---|
| Upset | the car is in trouble: airborne for 0.3 s, or tilted past 60° |
| Save | after an upset, back on four wheels, level and still for 0.5 s |
| Crash | a hard body or roof hit, a roof contact upside down, stuck on its side, or out of bounds |
| Hard landing | a wheel bottoms out (compression ≥ 0.95) within 0.2 s of touchdown |
| Tilt | the angle between the car's up axis and world up (0° upright, 180° inverted) |
| Hop | a short burst on all four lift nozzles, to jump over obstacles |
| Boost | both rear nozzles pushing forward, faded out between 50 and 60 m/s |
| Interlock | cuts the controller's thrust while the car is steady on the ground |
| Flip plate | a kick plate whose impulse flips or rolls the car; strong ones turn it past 120° |
| Valve lag | the first-order delay (τ = 30 ms) between a commanded and an actual throttle |
| Tank gate | the factor (0 to 1) that fades thrust over the last 2% of the air |
| Allocation | turning a wanted force and torque into ten throttles in [0, 1] |
| Hybrid gate | the latch that hands the nozzles from PD to the AI during an upset, and back after |
| Quiet throttles | a throttle sum below 0.05 (ignoring nozzles under 0.02), needed for success |
| Golden trace | a scripted 2000-step run whose hash must be identical natively and in wasm |
| Schema hash | the FNV-1a hash of the observation layout, checked before any policy is used |
| Cell | one stage × mode (× plain or randomized) combination in an eval table |
