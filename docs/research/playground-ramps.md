# Playground ramps: airtime for flips and barrel rolls

The playground (`courses/playground.json`) is hand-made for manual stunts. Its jump ramps used to be 8 degree, 15 m ramps (airtime 1.2 to 1.8 s), which cannot fit a back flip: the planner needs 2.64 s for a flip and 2.49 s for a barrel roll (`docs/research/stunt-planning.md`, rule 2). The ramps are now 14 to 28 degrees, each with a landing pad, spread 60 to 120 m apart. Each ramp keeps the 110 m concave radius, so its length is 110 m times its angle in radians (22 degrees is 42 m, 28 degrees is 54 m).

Test and tool: `cargo test --release -p nl-control --test playground_stunts -- --ignored --nocapture` prints the table below (one run per row and column, Realistic mode, a clean level approach at the given speed, PD flying, 30 m run-up). `cargo run --release -p nl-control --example stunt_budget` measures the nozzles-off airtime of the same ramps (it rewrites `results/stunt_budget.json`).

How the player is modelled: the stunt key is pressed at the first moment `authorize` would say yes. "Pressed at once" is that moment right after take-off. "Pressed once level" also waits until PD has levelled the car to within 5 degrees (about 0.6 s on a 26 to 28 degree ramp). "req" is the plan's `required_s`, "forecast" the forecast time to touchdown at the press, and the last number the wait after take-off in seconds.

| Lip angle | Speed m/s | Airtime s (nozzles on, no stunt) | Back flip, pressed at once | Back flip, pressed once level | Barrel roll, pressed at once |
|---|---|---|---|---|---|
| 14 | 20 | 1.68 | refused | refused | refused |
| 14 | 25 | 1.93 | refused | refused | refused |
| 14 | 30 | 2.18 | refused | refused | refused |
| 14 | 35 | 2.43 | refused | refused | refused |
| 14 | 40 | 2.65 | refused | refused | refused |
| 16 | 20 | 1.96 | refused | refused | refused |
| 16 | 25 | 2.23 | refused | refused | refused |
| 16 | 30 | 2.51 | refused | refused | refused |
| 16 | 35 | 2.80 | refused | refused | refused |
| 16 | 40 | 3.03 | refused | refused | refused |
| 18 | 20 | 2.22 | refused | refused | refused |
| 18 | 25 | 2.54 | refused | refused | refused |
| 18 | 30 | 2.87 | refused | refused | refused |
| 18 | 35 | 3.15 | refused | refused | req 2.49 s, forecast 2.64 s, landed |
| 18 | 40 | 3.45 | req 2.64 s, forecast 2.93 s, landed* | refused | req 2.49 s, forecast 2.93 s, landed |
| 20 | 20 | 2.58 | refused | refused | refused |
| 20 | 25 | 2.87 | refused | refused | refused |
| 20 | 30 | 3.21 | refused | refused | req 2.49 s, forecast 2.62 s, landed |
| 20 | 35 | 3.52 | req 2.64 s, forecast 2.92 s, landed* | refused | req 2.49 s, forecast 2.92 s, landed |
| 20 | 40 | 3.83 | req 2.64 s, forecast 3.24 s, landed* | refused | req 2.49 s, forecast 3.24 s, landed |
| 22 | 20 | 2.86 | refused | refused | refused |
| 22 | 25 | 3.18 | refused | refused | refused |
| 22 | 30 | 3.53 | req 2.64 s, forecast 2.86 s, FAILED | refused | req 2.49 s, forecast 2.86 s, landed |
| 22 | 35 | 3.73 | req 2.64 s, forecast 3.20 s, FAILED | refused | req 2.49 s, forecast 3.20 s, landed |
| 22 | 40 | 0.00 CRASH | req 2.64 s, forecast 3.48 s, FAILED | req 2.64 s, forecast 2.96 s, FAILED | req 2.49 s, forecast 3.48 s, landed* |
| 24 | 20 | 3.11 | refused | refused | refused |
| 24 | 25 | 3.49 | req 2.64 s, forecast 2.77 s, FAILED | refused | req 2.49 s, forecast 2.77 s, landed |
| 24 | 30 | 3.87 | req 2.64 s, forecast 3.12 s, landed* | refused | req 2.49 s, forecast 3.12 s, landed |
| 24 | 35 | 4.23 | req 2.64 s, forecast 3.47 s, landed* | req 2.64 s, forecast 2.94 s, landed | req 2.49 s, forecast 3.47 s, landed |
| 24 | 40 | 4.57 | req 2.64 s, forecast 3.84 s, landed* | req 2.64 s, forecast 3.30 s, landed | req 2.49 s, forecast 3.84 s, landed |
| 26 | 20 | 3.38 | refused | refused | req 2.49 s, forecast 2.62 s, landed |
| 26 | 25 | 3.80 | req 2.64 s, forecast 2.98 s, landed* | refused | req 2.49 s, forecast 2.98 s, landed |
| 26 | 30 | 4.21 | req 2.64 s, forecast 3.34 s, landed* | req 2.64 s, forecast 2.80 s, landed | req 2.49 s, forecast 3.34 s, landed |
| 26 | 35 | 4.58 | req 2.64 s, forecast 3.72 s, landed* | req 2.64 s, forecast 3.19 s, landed | req 2.49 s, forecast 3.72 s, landed |
| 26 | 40 | 4.95 | req 2.64 s, forecast 4.10 s, landed* | req 2.64 s, forecast 3.58 s, landed | req 2.49 s, forecast 4.10 s, landed |
| 28 | 20 | 3.67 | req 2.64 s, forecast 2.77 s, landed* | refused | req 2.49 s, forecast 2.77 s, landed |
| 28 | 25 | 4.08 | req 2.64 s, forecast 3.15 s, landed* | refused | req 2.49 s, forecast 3.15 s, landed |
| 28 | 30 | 4.52 | req 2.64 s, forecast 3.52 s, landed* | req 2.64 s, forecast 3.03 s, landed | req 2.49 s, forecast 3.52 s, landed |
| 28 | 35 | 4.93 | req 2.64 s, forecast 3.93 s, landed* | req 2.64 s, forecast 3.44 s, landed | req 2.49 s, forecast 3.93 s, landed |
| 28 | 40 | 5.30 | req 2.64 s, forecast 4.34 s, landed* | req 2.64 s, forecast 3.82 s, landed | req 2.49 s, forecast 4.34 s, landed |

## What the numbers say
- **Barrel rolls** authorize and land (`Landed` event) from 18 degrees at 35 m/s, 20 degrees at 30 m/s, 24 degrees at 25 m/s and 28 degrees at 20 m/s. Below that the forecast minus its uncertainty is under 2.49 s and the planner says no, which is correct: a roll there would not finish.
- **Back flips** need a bit more: pressed at once they turn all the way and land on four wheels from 24 degrees at 30 m/s, but no `Landed` event follows. A ramp launches the car nose up by about the lip angle; after a full turn the car is nose up again, PD's catch levels it, and that undoes part of the turn the planner counts, so it ends below 2 pi minus the slack and reports "not finished". Pressed once PD has levelled the car (the table's fifth column) the flip lands with a `Landed` event from 24 degrees at 35 m/s, 26 degrees at 30 m/s and 28 degrees at 30 m/s. Fixing the planner to remember a completed turn would make the early press count too; that is a change to `crates/nl-control/src/stunt.rs`, not made here.
- **No crashes** on a clean level approach under PD at 20 to 35 m/s on any ramp (0 of 32 runs, against 0 of 4 on the old 8 degree ramp). At 40 m/s the 22 degree ramp throws the car 175 m and the plain PD landing is a hard impact. With the nozzles off (`stunt_budget`) the steep ramps crash on landing by design; they are for the thrusters.
- Landing air with PD rises with the lip: about 25 kg at 14 degrees and 20 m/s, 52 kg at 22 degrees, 70 kg at 28 degrees and 20 m/s, 85 kg at 28 degrees and 30 m/s. In Limited air the tank has to cover that plus the stunt.

## Why the playground may exceed the 8 degree cap
`docs/contracts/ramp-curvature.md` caps lips at 8 degrees for jump ramps because training and evals (Stage D) must be survivable by clean entries up to 55.6 m/s. The playground is not in `physics_hash`, is not used by any training stage, eval, curve or the golden trace, and nobody is scored on it, so it may go to 28 degrees. `piece_geometry.rs` pins 8 degrees for generated courses and 28 degrees with the 110 m radius for the playground.
