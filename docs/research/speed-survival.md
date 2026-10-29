# Speed survival and drive research (R2)

Research stream R2 of the stunts/speed/viewer plan (`docs/plans/2026-10-27-stunts-speed-viewer.md`). No gameplay change: it measures, and proposes values for G2 (`claude/roadster-drive`, `claude/fearless-pieces`). All numbers come from the tools in this PR and can be reproduced with the commands below. Results: `results/baseline_2026-10/speed_sweep.{json,csv}` (the G1 baseline under today's physics) and `results/speed_sweep_drive_candidate.{json,csv}`.

## 1. Drive values (R2a)

Target: 0–100 km/h in 2.0 s, top speed 103 m/s (371 km/h), steering and grip still usable at 100 m/s, wheels steady at 100 m/s. Tool: `cargo run --release -p nl-sim --example drive_target [-- vehicle.toml]` (Realistic, flat ground; it never edits the repo config).

| | today | candidate |
|---|---|---|
| `max_engine_force_per_wheel_n` | 3000 | 7200 |
| `max_engine_power_w` | 220 000 | 800 000 |
| `max_speed_m_s` | 55 | 103 |
| `boost_max_speed_m_s` | 60 | 110 |
| `friction_slip` | 1.3 | 1.9 |
| **0–100 km/h** | 4.58 s | **1.98 s** |
| 0–200 km/h | 43.2 s | about 4.9 s |
| top speed | 57.3 m/s (cap 55) | **103.0 m/s**, reached in 16 s |
| peak acceleration | 0.68 g | 1.63 g |
| launch traction share (1 = no wheelspin) | 1.00 | 0.78 |
| lateral g at 100 m/s, small steer | 1.28 | 1.67 |
| flat / floor seams / 3° ramp at 100 m/s | steady | steady: all wheels down, no dropouts, no body touch |

How the numbers were found: the launch needs about 25 kN (0.68 g → 1.4 g and more), which needs grip above 1.4, and holding 103 m/s against about 3.8 kN of drag needs under 400 kW, so 800 kW leaves a margin for the launch. A first guess (7000 N, 750 kW, grip 1.8) gave 2.03 s; 7200 N, 800 kW and grip 1.9 gives 1.98 s. Grip 2.1 reaches 1.94 s but changes lateral grip more than needed.

**Watch in G2/G3:** the AI's observation clamps speed at 60 m/s (the Hybrid hands over to PD above 55 m/s, plan §1.6 `PolicyOutOfEnvelope`); `friction_slip` also raises lateral grip, so steering and the PD gains need a retune (G3); mild launch wheelspin remains (0.78).

## 2. Suspension limit and bump stop (R2b)

Tool: `cargo run --release -p nl-sim --example suspension_load [-- vehicle.toml]`. A concave curve rig is driven at held speeds; the design load at the foot is n = v²/R + g, and the lowest speed at which the body touches the ground gives the measured limit `n_max`.

**Stock suspension:** spring-only capacity is 3.87 g at the body-touch compression (0.19 m of 0.20 m travel). On the curve rig (R = 28.6 m) the body first touches at 28 m/s, a design load of **3.79 g**; the last clean run is 26 m/s, **3.41 g**. The contract assumed 3 g total load, so the real limit is a little higher.

**Bump stop (prototype, not committed):** a stiff spring over the last part of the travel, `extra force = rate × mass × (compression − start)`, capped by `max_suspension_force_n`. Swept with the curve rig and the flat-drop check (the rebound after a drop must stay below the drop height; the smallest drop is 0.25 m):

| start | rate (1/s²) | last clean design load | first body touch | highest clean flat drop | max rebound |
|---|---|---|---|---|---|
| none | 0 | 3.41 g (26 m/s) | 3.79 g (28 m/s) | 1.00 m | 0.006 m |
| 0.14 m | 100 | 3.41 g (26 m/s) | 3.79 g (28 m/s) | 1.00 m | 0.007 m |
| 0.14 m | **300** | **4.21 g (30 m/s)** | 4.65 g (32 m/s) | 1.00 m | 0.087 m |
| 0.14 m | 1000 | 4.65 g (32 m/s) | 5.37 g (35 m/s) | 1.00 m | 0.257 m: exceeds the 0.25 m drop |
| 0.14 m | 3000 | 4.65 g (32 m/s) | 5.37 g (35 m/s) | 1.00 m | 0.265 m: exceeds the 0.25 m drop |
| 0.16 m | 300 | 3.41 g (26 m/s) | 3.79 g (28 m/s) | 1.50 m | 0.007 m |
| 0.16 m | 1000 | 3.41 g (26 m/s) | 3.79 g (28 m/s) | 1.50 m | 0.053 m |
| 0.16 m | 3000 | 3.79 g (28 m/s) | 4.21 g (30 m/s) | 1.00 m | 0.123 m |

**Recommendation:** start 0.14 m (70% of travel), rate 300 per second squared. It lifts the clean design load from 3.4 g to **4.2 g** with a rebound of 0.087 m. Stiffer stops add little load and bounce the car. G2 should build it as proper `[wheels]` settings (`bump_stop_start_m`, `bump_stop_rate_per_kg`) and re-measure with this tool.

**What it means for ramps** (concave entry radius R = v²/((n − 1) g) at 200 km/h, 55.6 m/s; the curved part of an 18° ramp is about R × 0.314 rad):

| design load n | R | curved length, 18° ramp |
|---|---|---|
| 3.0 g (the contract) | 158 m | 49 m |
| 3.4 g (stock, measured) | 137 m | 43 m |
| 4.2 g (with the bump stop) | 99 m | 31 m |

The crest radius (mid-ramp convex curve, about 315 m so the car never goes light) does not depend on the suspension.

## 3. Per-piece speed sweep (R2c)

Tool: `cargo run --release -p nl-control --example speed_sweep -- --seeds 20 --threads 6 --out results/baseline_2026-10/speed_sweep` (13 s). It drives the car, level, aligned and already rolling, onto one piece at a time at 24 entry speeds from 2 to 55.7 m/s, with PD flying the thrusters, in both modes over 20 seeds (seed 0 dead centre, the rest inside the clean-approach limits). Each crash is traced to the contact that caused it. **Today's physics: 16320 runs, 1295 crashes (7.9%).**

### Why cars crash

| failure class | crashes | what it is | G2 lever |
|---|---|---|---|
| `concave_bottoming` | 598 | the body hits the ground at the foot of a curved piece (kicker, corkscrew, wall ramp) | bump stop and longer entry curves (§2) |
| `bump_face` | 413 | the car hits the face of a 0.4 m or 0.5 m bump | bump height and spacing, and the approach rules |
| `other_floor_contact_artifact` | 244 | floor contact after flight or on a ramp that the crash rule counts as a crash (ramps, flips) | the scrape-versus-crash safeguards (`docs/contracts/crash-scrape.md`) |
| `other_fell_in_gap` | 40 | the car enters a gap piece too slowly to clear it | the `v_complete_min_m_s` fallback (`docs/contracts/survival.md`) |

### Crashes per piece (today / with the drive candidate)

| piece | runs | crashes today | crashes with candidate drive | lowest crash speed, Realistic | lowest crash speed, Arcade |
|---|---|---|---|---|---|
| `ramp_15` | 960 | 19 | 10 | 40 m/s | 30 m/s |
| `ramp_18` | 960 | 21 | 24 | 30 m/s | 25 m/s |
| `ramp_22` | 960 | 23 | 20 | 9.9 m/s | 30 m/s |
| `ramp_28` | 960 | 32 | 22 | 12.5 m/s | 9.9 m/s |
| `ramp_gap` | 960 | 45 | 44 | 5 m/s | 5 m/s |
| `kicker` | 960 | 118 | 108 | 30 m/s | 30 m/s |
| `bumps_0.2` | 960 | 0 | 0 | none | none |
| `bumps_0.4` | 960 | 151 | 153 | 42.5 m/s | 22.5 m/s |
| `bumps_0.5` | 960 | 284 | 251 | 10 m/s | 10 m/s |
| `corkscrew` | 960 | 330 | 352 | 17.5 m/s | 12.5 m/s |
| `halfpipe` | 960 | 0 | 0 | none | none |
| `wall_ramp` | 960 | 200 | 192 | 12.5 m/s | 17.5 m/s |
| `flip_front_flip` | 960 | 17 | 15 | 30 m/s | 27.5 m/s |
| `flip_back_flip` | 960 | 26 | 20 | 20 m/s | 15 m/s |
| `flip_barrel_left` | 960 | 0 | 0 | none | none |
| `flip_barrel_right` | 960 | 0 | 0 | none | none |
| `flip_roof_toss` | 960 | 29 | 28 | 12.5 m/s | 17.5 m/s |

**Findings**

- Today, only `bumps_0.2`, the halfpipe and the barrel flips never crash. Everything else crashes at some speed, often well under 200 km/h (the corkscrew from 12.5 m/s, bumps 0.5 from 10 m/s).
- The stronger drive changes survival very little (1239 crashes against 1295), because the sweep fixes the entry speed. Survival is decided by geometry, suspension travel and the crash rules, not by drive strength.
- The largest groups, concave bottoming (598) and bump faces (413), need the bump stop and geometry changes of G2; the floor-contact group (244) mostly needs the scrape rule rather than physics changes.

## 4. Reproduce

```
cargo run --release -p nl-sim --example drive_target -- path/to/vehicle.toml
cargo run --release -p nl-sim --example suspension_load -- path/to/vehicle.toml
cargo run --release -p nl-control --example speed_sweep -- --seeds 20 --threads 6 --out results/baseline_2026-10/speed_sweep
cargo run --release -p nl-control --example speed_sweep -- --seeds 20 --threads 6 --vehicle path/to/vehicle.toml --out results/speed_sweep_drive_candidate
```
The candidate vehicle file is `configs/vehicle.toml` with the five drive values from §1. The bump-stop prototype was a local, uncommitted change to `car.rs` read from environment variables; it is described here, not shipped.
