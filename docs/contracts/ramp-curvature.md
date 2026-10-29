# Contract: ramp curvature

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.4). Applied to piece geometry in G2 (`claude/fearless-pieces`).

## Why
Today's ramp curves (radius 31–39 m) bottom out the 0.2 m suspension at 31–34 m/s. At 200 km/h the curves must be gentle enough that the suspension (with its new bump stop) can carry the load.

## Symbols
- `v`: car speed in m/s. The design speed is **55.6 m/s** (200 km/h).
- `R`: radius of the curve in m.
- `θ`: slope angle of the surface at that point.
- `g`: 9.81 m/s².
- `n`: suspension load per unit mass, in m/s².
- `n_max`: the largest load the suspension can sustain with the bump stop. R2b measures it; expected about **3 g total** (about 29.4 m/s²).

## Concave entry transition (the ramp bends up)
The load is `n = v²/R + g·cosθ`. It must stay at or under `n_max`, so:

```text
R ≥ v² / (n_max − g·cosθ)      at v = 55.6 m/s
```

Example with `n_max` = 3 g on flat ground (θ = 0): R ≥ 55.6² / (29.4 − 9.81) ≈ 158 m.

## Convex lip and crests (the surface bends down)
- The car goes light when `v²/R > g·cosθ`.
- **At the lip:** allowed. The car takes off on purpose.
- **Mid-piece crests:** must satisfy `v²/R ≤ g·cosθ`, so the car never leaves the ground by accident.

Example on flat ground at 55.6 m/s: a mid-piece crest needs R ≥ 55.6² / 9.81 ≈ 315 m.

## Kinks
The piece is built from a polyline. The angle between two neighbouring segments must be **≤ 1.5°**.

## Tests that pin this (plan §3)
- **Survival:** the 0-crash clean-entry sweep up to 55.6 m/s (`survival.md`).
- **Contact robustness:** wheel ray contact over seams and kinks at 100 m/s.
- **Bump stop:** energy is not gained (rebound ≤ compression over a drop test); no oscillation growth over 10 s of bump-strip driving at 55 m/s.
- A geometry unit test over every piece: concave radius, crest radius and kink angle within the limits above.
