# Contract: ramp curvature

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.4). Applied to piece geometry in G2 (`claude/fearless-pieces`). **G2 update (2026-10-29):** `n_max` was measured at 4.2 g, not 3 g, so the concave radius is 110 m, not 158 m (see below).

## Why
Today's ramp curves (radius 31–39 m) bottom out the 0.2 m suspension at 31–34 m/s. At 200 km/h the curves must be gentle enough that the suspension (with its new bump stop) can carry the load.

## Symbols
- `v`: car speed in m/s. The design speed is **55.6 m/s** (200 km/h).
- `R`: radius of the curve in m.
- `θ`: slope angle of the surface at that point.
- `g`: 9.81 m/s².
- `n`: suspension load per unit mass, in m/s².
- `n_max`: the largest load the suspension can sustain with the bump stop. R2b measured it (`docs/research/speed-survival.md` §2, re-measured in G2 with `cargo run --release -p nl-sim --example suspension_load`): the last clean run is **4.21 g** (30 m/s on the 28.6 m rig), the first body touch 4.65 g, with the bump stop `[wheels] bump_stop_start_m = 0.14`, `bump_stop_rate_per_kg = 300`. The Gate 0 estimate was 3 g; without the stop the limit is 3.4 g.

## Concave entry transition (the ramp bends up)
The load is `n = v²/R + g·cosθ`. It must stay at or under `n_max`, so:

```text
R ≥ v² / (n_max − g·cosθ)      at v = 55.6 m/s
```

Example with `n_max` = 4.2 g on flat ground (θ = 0): R ≥ 55.6² / (41.2 − 9.81) ≈ 98.5 m.

**Radius chosen: `R = 110 m`** (`course_build::MIN_CONCAVE_RADIUS_M`, `configs/pieces.json` `design.min_concave_radius_m`). It is a design load of 3.9 g at 55.6 m/s, 7% under the measured clean limit, so a 99 m radius that sits exactly on the limit is avoided. A curved piece is `R · θ` long (parabola: h = R·θ²/2; corkscrew low edge: `R · tan θ`).

## Convex lip and crests (the surface bends down)
- The car goes light when `v²/R > g·cosθ`.
- **At the lip:** allowed. The car takes off on purpose.
- **Mid-piece crests:** must satisfy `v²/R ≤ g·cosθ`, so the car never leaves the ground by accident.

Example on flat ground at 55.6 m/s: a mid-piece crest needs R ≥ 55.6² / 9.81 ≈ 315 m.

## Kinks
The piece is built from a polyline. The angle between two neighbouring segments must be **≤ 1.5°**. G2 builds them at **≤ 1.2°** (`MAX_KINK_DEG`, more segments for steeper ramps), leaving rounding margin.

## Lip height and landing (added in G2)
A car leaving a lip at 55.6 m/s lands with a fall speed of √((v·sin θ)² + 2·g·H), where H is the lip height. R2c's sweep (12° lips, H ≈ 2.4 m, 13 m/s) crashed a few landings; 8° lips (H ≈ 1.1 m, 9 m/s) did not. So generated and training jump ramps, kickers and corkscrews use lip angles of **8° or less**. A taller piece must be a table top (a wall with a ramp is one: ramp, deck, ramp back down).

**The playground is the exception.** `courses/playground.json` is hand-made for manual back flips and barrel rolls, which need 2.5 s or more of air (`docs/research/playground-ramps.md`), and nothing trains or scores on it. Its ramps, kickers and corkscrews keep the 110 m concave radius (length = 110 m × lip angle in rad) and the kink limit, but their lips go from 14° to **28°** (`MAX_PLAYGROUND_LIP_DEG` in `piece_geometry.rs`). A clean level approach under PD still lands them without a crash at 20–35 m/s (`crates/nl-control/tests/playground_stunts.rs`).

## Tests that pin this (plan §3)
- **Survival:** the 0-crash clean-entry sweep up to 55.6 m/s (`survival.md`).
- **Contact robustness:** wheel ray contact over seams and kinks at 100 m/s.
- **Bump stop:** energy is not gained (rebound ≤ compression over a drop test); no oscillation growth over 10 s of bump-strip driving at 55 m/s.
- A geometry unit test over every piece: concave radius, crest radius and kink angle within the limits above.
