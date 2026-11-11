# Contract: g-meter

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.7). Built in A1 (`g-peak-sim`, sim side) and G5 (`g-meter-web`, web side). It replaces the screen blackout (`web/src/hud/g-blackout.ts`, since removed), which is removed.

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

**Rebound (decided 2026-11-02):** a hard landing can rebound off the ground since the G2 bump stop. A take-off inside the 0.5 s tail **continues** the window, so the landing's peak is kept; only a take-off after the window has closed (or a respawn or placement) resets the peak (`EventTracker::update_airborne`).

## Headline peak (web only, UI note 2026-11-03)
The raw peak can be a single 120 Hz contact tick (a drop test reads about 39 g), which says more about the solver than about the car. The g-meter's **headline** peak is the largest displayed (0.1 s smoothed) g since the raw peak last reset, **capped by the raw peak** (`web/src/ui/g-meter.ts`, `updateGPeak`). The raw peak is unchanged: `HudField` 23, the Save event's `peak_g`, and the stunt landing event all stay raw. The raw value shows in the peak label's tooltip. No sim or wasm change.

## Best-ever record (removed 2026-11-05)
The web no longer keeps or shows a best-g record or a per-stunt best line, and toasts carry no g number. The `ninelives.g.v1` key is not read or written; an old value in a browser is harmless.

## Tests that pin this (plan §3)
- Touchdown is included in the peak.
- The peak resets at the next take-off and on respawn.
- Raw and displayed peak are both reported.
- An old or unknown `localStorage` shape is dropped safely.
- **Browser check:** the g-meter screenshotted in the PR.
