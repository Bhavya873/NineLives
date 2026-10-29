# Contract: survival ("never crash, may not complete")

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.1). Every later PR builds to this. Changing it needs the owner and a line in `docs/DECISIONS.md`.

## The rule
A **clean entry** (see `traversal.md`) at any speed above 0 and up to **200 km/h (55.6 m/s)** must never crash the car *because it went over the obstacle*.

Clearing a gap, loop, jump or stunt may need a minimum entry speed. Below that speed the piece gives a **safe fallback**. It never gives a crash.

In short: the car may fail to complete a piece, but it never crashes on one it entered cleanly.

## Minimum completion speed
- Each piece will publish `v_complete_min_m_s` (m/s): the slowest clean entry that still completes it.
- Before entry, the HUD shows "too slow to clear this gap" when the car is below it (`HudField` 26 `ApproachHint`, event 16 `TooSlowFallback`; see `events-hud.md`).
- **Where it lives:** planned for `configs/pieces.json`. It is **not added yet**: `PieceCatalog` and `PieceTemplate` in `crates/nl-sim/src/course_gen.rs` use `deny_unknown_fields`, so a new field would break loading today. The G2 PR (`claude/fearless-pieces`) adds the field and its parser together. Hand-made courses (`courses/*.json`, also strict in `crates/nl-sim/src/course.rs`) get the same field in that PR.

## Fallbacks by piece
| Piece | Too slow to complete means |
|---|---|
| Ramp gap | The car drops into a catch pit with a sloped floor and an exit ramp. It can drive in and out. |
| Half loop | The car rolls back down the entry, still upright. |
| Jump | The landing zone covers every arc from `v_complete_min_m_s` to 200 km/h. Shorter arcs land on a flat, safe run-out. |
| Wall with ramp | The ramp leads to a top deck with a down-ramp. Not clearing the wall means stopping on the ramp, not hitting the wall face. |
| Corkscrew, halfpipe | Too little speed rolls the car back. |

## Not covered
- Driving straight into a wall face or a row of parked cars without using its ramp. That is not a clean entry and still crashes.
- Any entry above 200 km/h. There is no promise there, and the speed readout turns amber.

## Tests that pin this (plan §3)
- **Survival:** clean entry on every covered piece at 2, 5, 9.9, 10, 20, 30, 40, 50, 55.5 and 55.6 m/s (55.7 m/s is logged only), 20 seeds, both modes, PD flying: 0 crashes.
- **Fallback:** below `v_complete_min_m_s`, the fallback is taken and is safe (half loop: rolls back upright).
- **Contact robustness:** obstacle positions shifted by 0, ¼, ½ and ¾ of one tick's travel (v·dt) give the same result.
- **Browser check:** every playground piece taken flat out at about 190 km/h, and each too-slow fallback, screenshotted in the PR.
