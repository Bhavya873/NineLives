# Contract: g-meter

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.7). Built in A1 (`claude/g-peak-sim`, sim side) and G5 (`claude/g-meter-web`, web side). It replaces the screen blackout (`web/src/hud/g-blackout.ts`), which is removed.

## Raw and displayed g
- **Raw g:** the seat g-load (SPEC §9.4, `crates/nl-sim/src/g_load.rs`) at every 120 Hz physics tick, **unsmoothed**.
- **Displayed g:** the same value smoothed over **0.1 s**. This is what the meter needle shows.
- **Peak g:** the running maximum of the **raw** g. It goes in `HudField` 23 `PeakGLoad` and in the Save event's 5th float `peak_g` (see `events-hud.md`).

## Stunt window (when the peak is collected)
- **Starts** at `AirborneStart`, or at `StuntStart` if a stunt begins first.
- **Ends** 0.5 s after the touchdown that ends the airtime.
- **Touchdown counts.** Landing g is part of the stunt.

## Reset
The peak resets to 0 at the next `AirborneStart`, at a respawn, or at a placement.

**Known behaviour, owner to decide (2026-10-29):** since the G2 bump stop, a hard landing can rebound off the ground. The rebound is a new take-off (`AirborneStart`), which opens a fresh window even inside the 0.5 s tail and drops the landing's peak: after a 3 m drop the raw peak reads 1.4 g although the landing reached about 5.8 g (smoothed). The tests read the peak tick by tick to see the landing. If the meter should keep the landing, a take-off inside the 0.5 s tail should continue the window instead of resetting it (one change in `EventTracker`).

## Best-ever record (web only)
- `localStorage` key `ninelives.g.v1`, value `{ bestPeakG, version }`.
- Every read and write is wrapped in try/catch (CLAUDE.md gotcha).
- An unknown or older shape is dropped and a fresh record starts. That is the migration path.

## Tests that pin this (plan §3)
- Touchdown is included in the peak.
- The peak resets at the next take-off and on respawn.
- Raw and displayed peak are both reported.
- An old or unknown `localStorage` shape is dropped safely.
- **Browser check:** the g-meter screenshotted in the PR.
