# Contract: event and HUD numbering

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.6). The numbers are reserved now so parallel PRs do not collide. They are also listed in a comment above `HudField` in `crates/nl-wasm/src/lib.rs`. `EventKind` 15 and 16 exist since G2 (`Scrape`: `[15, normal_speed_m_s, 0, 0, 0]`; `TooSlowFallback`: `[16, piece_index, entry_speed_m_s, 0, 0]`).

## HudField (the array from `Game::hud`)
Today the last field in use is 27 `PolicyOutOfEnvelope` (G3); 24 to 26 are still reserved by other streams and read 0, so the array has 28 values.

| Number | Name | Meaning | Added by |
|---|---|---|---|
| 23 | `PeakGLoad` | Raw peak g of the current stunt window, in g (see `g-meter.md`) | A1 `claude/g-peak-sim` |
| 24 | `StuntPhase` | The stunt phase as a number (see `stunt-states.md`) | G4 |
| 25 | `StuntCode` | Which stunt: barrel left/right, front flip, back flip | G4 |
| 26 | `ApproachHint` | What the approach HUD should say, e.g. "too slow to clear this gap" | G2 / G5 |
| 27 | `PolicyOutOfEnvelope` | 1 when the AI is outside where it was validated (speed > 55 m/s or another observation clamp), so the Hybrid uses PD | G3 (in use; set under any controller, computed by `nl_control::PolicyEnvelope`, 0 while the wreck is frozen; the limit is `[hybrid] policy_max_speed_m_s`) |

## EventKind (the first float of each event from `Game::drain_events`)
Today the last kind is 10 `HardLanding`.

| Number | Name | Meaning |
|---|---|---|
| 11 | `StuntStart` | An authorized stunt began SpinUp |
| 12 | `StuntLanded` | The stunt reached Settled with full rotation |
| 13 | `StuntRefused` | The planner said no to a request |
| 14 | `StuntAborted` | The planner stopped a stunt mid-air and handed over to PD |
| 15 | `Scrape` | An underbody touch classed as a Scrape, not a crash (see `crash-scrape.md`) |
| 16 | `TooSlowFallback` | The car entered a piece below `v_complete_min_m_s` and takes the fallback (see `survival.md`) |

## Save event gets a 5th float
Each event is 4 floats today (`FLOATS_PER_EVENT = 4`): the kind, then up to three values. Save is `[Save, duration_s, air_used_kg, peak_tilt_rad]`.

A1 makes every event **5 floats**, and Save becomes `[Save, duration_s, air_used_kg, peak_tilt_rad, peak_g]`. Other events pad the extra float with 0. The web reader changes in the same PR.

## Tests that pin this (plan §3)
- **G-meter:** Save carries `peak_g`; raw and displayed peak both reported.
- **Ownership:** stunt phase written to the HUD and the event stream.
- **AI envelope:** above 55 m/s the `PolicyOutOfEnvelope` flag is set and the Hybrid uses PD.
- `npm run test:wasm` (Game API tests) checks the HUD length and event width after each change.
