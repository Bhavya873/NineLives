# Control upgrade audit

What the thruster controller sees and drives today. This is the first step of the
"Targeted Control Improvements" brief: every later change is checked against this map and
the legacy PD baseline in `results/control_upgrade/`. It describes the code at branch
`claude/control-audit` (based on the realistic-drive commit `6d91dfb`). Paths are from the
repo root; symbols are `file::item`.

## Conventions

| Topic | Where | What the code does |
|---|---|---|
| Axes | `crates/nl-sim/src/conventions.rs`, SPEC §4 | SI units. World +Y up. Body frame +X forward, +Y up, +Z right. |
| Rotation | Rapier 0.36 (`glam` types), `world.rs::Pose` | Unit quaternion, body → world: `world_vector = rotation * body_vector`. `World::in_body_frame` uses `rotation.inverse()`. |
| Angular velocity | `World::angular_velocity_body_rad_s`, `CarState::angular_velocity_body_rad_s` | Rapier stores it in the world frame; controllers get it in the body frame: roll about +X, yaw about +Y, pitch about +Z. |
| Timesteps | `conventions.rs::PHYSICS_DT_S` (1/120 s), `PHYSICS_STEPS_PER_CONTROL_STEP` (2) | Controllers run at 60 Hz. `World::is_control_step` is the only test of that, used by the game, eval and training. Controller throttles are held for both physics steps (`World::set_controller_throttles`). |
| Nozzle order | `conventions.rs`, `configs/vehicle.toml` `[[thrusters.nozzles]]` | `L_FL L_FR L_RL L_RR S_FL S_RL S_FR S_RR B_L B_R`. Lift nozzles 0–3, side 4–7, boost 8–9 (`hop_boost.rs::LIFT_NOZZLES`, `BOOST_NOZZLES`). |
| Nozzle geometry | `vehicle_config.rs::NozzleConfig` | Position (body frame, m), push direction (unit), `max_force_n` at Realistic scale. |
| Centre of mass | `vehicle_config.rs::ChassisConfig::center_of_mass_offset_m`, `car.rs::build_chassis_body` | 0 in `vehicle.toml`. Domain randomization moves it up to ±0.1 m per axis (`configs/scenarios.toml` `[randomization]`, `randomization.rs`). |

## One physics step: `World::step_car` (`crates/nl-sim/src/world.rs`)

The order matters for any predictor, so here it is exactly:

1. `HopBoost::update(hop_just_pressed, boost_held, wheels_down, air_kg, mode, dt)`: timers
   tick down, then a new Hop starts if the button went down this step and
   `hop_blocked_reason` allows it.
2. Feed-forward: `HopBoost::feed_forward_throttles(mode, boost_share)`, where
   `hop_boost.rs::boost_speed_share(drive, forward_speed)` fades Boost out near
   `drive.boost_max_speed_m_s`.
3. Interlock: `World::thrusters_locked()` replaces the controller's throttles with zeros.
   The feed-forward is never cut. The test is upright, barely spinning, barely moving
   vertically and `height_above_ground_m <= max_height_m`. That height is a downward ray
   from the centre of mass, so it is terrain-relative, not altitude.
4. `hop_boost.rs::combine_throttles(ff, ctrl)`: `clamp(ff + safe_throttle(ctrl), 0, 1)`
   per nozzle (SPEC §7.7). A controller can only add on top of Hop and Boost.
5. Tank gate: `World::thrust_multiplier()` = `mode.thrust_scale × AirTank::thrust_gate`,
   read from the tank **before** this step's air is paid.
6. `Wind::step`.
7. `Car::prepare_physics_step` (`car.rs`): driver input, then
   `Valves::update(commanded, dt, valve_time_constant_s)`, then forces reset and drag,
   then `thrusters.rs::apply_nozzle_forces(body, thrusters, effective, multiplier)`, then
   wheels. It returns the summed force magnitude.
8. `physics.step()`.
9. `AirTank::use_air(total_force_n, exhaust_speed, dt, mode)`, then
   `AirTank::refill(wheels_down_after, dt, mode)`. Refill counts the wheels **after** the
   physics step and waits `refill_delay_s` after the last use.
10. Course triggers, then events (crash and save rules).

## Actuators

| Topic | Where | Detail |
|---|---|---|
| Valve lag | `thrusters.rs::Valves::update`, `valve_step_fraction` | `u_eff += (safe(u_cmd) − u_eff)·(1 − exp(−dt/τ))`, with `libm::expf`. τ = `thrusters.valve_time_constant_s` = 0.03 s. Over one control interval (two physics steps) the mean opening moves only about a third of the way to a new command. |
| Nozzle force | `thrusters.rs::apply_nozzle_forces` | `F = direction × max_force × effective × multiplier`, applied with `add_force_at_point` at `pose * position`. Rapier turns that into torque about the **true** centre of mass. |
| Tank | `tank.rs::AirTank` | Mass flow = force / exhaust speed (600 m/s). `thrust_gate` slides linearly from 1 to 0 over the last `empty_fade_fraction` of capacity. It is always 1 with unlimited air. |
| Hop / Boost | `hop_boost.rs` | Hop puts `mode.hop_ff` on the lift nozzles for `mode.hop_s`. Boost puts `mode.boost_ff × boost_share` on the boost nozzles while held. |

## What a controller sees

| Topic | Where | Detail |
|---|---|---|
| Interface | `crates/nl-control/src/lib.rs::Controller`, `ControlCtx` | `act(&ControlCtx) -> [f32; 10]` and `reset(seed)`. `ControlCtx` holds the driver's `InputFrame`, a `CarState` and the 41-value observation. |
| Game | `crates/nl-wasm/src/lib.rs::Game::tick` | On a control step, `ControlCtx::for_world` (no noise, no latency). The controller resets every tick while crashed, and on `drop_test`, `load_course` and a controller switch. Buttons pressed between control steps are OR-ed in. |
| Eval / viewer | `crates/nl-control/src/eval.rs::step_episode` | `ControlCtx::for_episode(episode, InputFrame::default())`. PD never sees the driver bot's input; the bot's Hop and Boost reach the world directly (`Episode::driver_input`). |
| Training | `crates/nl-py/src/env_slot.rs::controller_ctx` | Builds a `ControlCtx` from `TrainingEnv`. `ResidualPd` (`residual.rs`) adds a policy residual on top of PD. |
| Noise | `crates/nl-sim/src/episode.rs::Episode::controller_view` | Adds sensor noise to a copy of `CarState` and consumes `noise_rng`. A predictor must never call it. |
| Latency | `Episode::step` | `delayed_throttles` (a queue), `commanded_throttles`, `applied_throttles`: under randomization the valves get the command from `latency_control_steps` ago. All of it lives in `Episode`, not `World`. |
| Observation | `crates/nl-sim/src/obs.rs::observe`, `OBS_DIM` = 41, `obs_schema_hash` | `prev_action` (rows 31–40) is the last **commanded** throttles, not the applied ones. |
| AI action | `crates/nl-policy/src/policy.rs::Policy::act`, `crates/nl-sim/src/training.rs::throttles_from_action` | Normalize, clip, MLP with tanh, clamp to [−1, 1], then `((a + 1)/2).clamp(0, 1)`; NaN counts as 0. |
| Hybrid | `crates/nl-control/src/hybrid.rs::HybridGate` | The AI takes over when `up_alignment < cos(enter_tilt)` and hands back after `release_hold_s` settled. PD acts on every step so its warm start stays current. |

## PD and the allocation (`crates/nl-control/src/pd.rs`, `allocation.rs`)

- `Pd::wanted_wrench_for(state, input)`: attitude torque = inertia × (kp·error − kd·spin),
  limited to `max_angular_accel_rad_s2`, plus vertical braking, lateral damping and the
  ground roll. It returns zero while driving on the wheels.
- `Pd::act` calls `Allocator::solve(wrench, state.mode_thrust_scale, &mut throttles)`. It
  plans with the mode's scale only and deliberately ignores the tank gate: dividing by it
  would drive every nozzle to full as the tank empties.
- `Allocator`: minimises `|W(B·u − w)|² + λ|u|²` with `0 ≤ u ≤ 1` by projected gradient.
  It is warm-started from the last answer, takes 30 steps of `1/(2(L + λ))` with
  Gershgorin `L`, and scales each row to 1 at its strongest nozzle, times `force_weight`
  or `torque_weight` (`configs/controllers.toml` `[allocation]`).
- `B` comes from `car_state.rs::read_car_model` via
  `thrusters.rs::nozzle_force_and_torque_at_full_throttle`: torque = `position × F`.

## Findings

1. **Torque is taken about the wrong point under randomization.** `read_car_model` uses
   torques about the body origin; Rapier applies them about the centre of mass. With the
   default car they agree (offset 0). Under domain randomization (±0.1 m) PD's `B` is off,
   and PD is built once from the base car anyway (`examples/eval.rs::build_named_controller`).
2. **The allocation ignores the actuator.** It treats `B·u` as the wrench that happens. In
   fact the valves move only about a third of the way in one interval, Hop and Boost sit
   underneath (the clamp leaves `1 − ff` headroom), the tank gate scales the result, and
   the interlock may cut it all. None of this is in the solve.
3. **Ground friction randomization does not change tyre grip.** Raycast wheels ignore
   collider friction (`World::new_on_flat_ground`); grip is `wheels.friction_slip` only,
   and it is never randomized.
4. **The interlock height is terrain-relative** (a ray from the centre of mass), not
   altitude.
5. **PD gets no driver input in eval and training** (`InputFrame::default()`), so its air
   wishes are never exercised there.
6. **Controller state is not in any snapshot.** `World::snapshot` (postcard, format 17)
   holds valves, tank and Hop/Boost. It does not hold PD's warm start, the Hybrid gate
   timers or the `Episode` latency queue. A restored world with a fresh controller does not
   replay bit-identically unless the controller is also reset at the same point.
7. **Pre-existing staleness.** The committed `results/` Stage D/E numbers before
   `6d91dfb` were older than the code (for example, PD D Realistic was committed at 99.5%,
   but the code before the drive change gives 98.1%). The new car changed every cell by
   1.1 points or less.
8. **PD's model has the yaw and pitch inertia swapped.** `vehicle.toml` gives
   `[850, 3250, 2900]` kg·m² (roll, yaw, pitch). `read_car_model` returns
   `principal_inertia()`, which Rapier lists in its own principal-axis order:
   `[850, 2900, 3250]`. So PD's attitude torque uses 3250 for pitch (12% too much) and 2900
   for yaw (11% too little). Found by `tests/actuator_model.rs`, which measures the car's
   real spin-up. It is left as it is, because fixing it changes the legacy controller.

## Baseline

The legacy PD baseline is in `results/control_upgrade/2026-10-26-legacy-pd/`, with
`manifest.json` holding the commit, config hashes, seeds and timing method, and
`baseline/failures.json` listing every seed that already fails. It covers stages A–F in
both modes.

Commands (from a scratch directory, because the runner writes `results/` into the current
directory):

```bash
cargo build --release -p nl-control --example eval
for stage in A B C D E F; do for mode in realistic arcade; do
  target/release/examples/eval --controller pd --stage $stage --mode $mode \
    --episodes 1000 --seed 1500000
done; done
```
