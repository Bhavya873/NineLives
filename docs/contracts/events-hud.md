# Contract: event and HUD numbering

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.6). The numbers are reserved now so parallel PRs do not collide. They are also listed in a comment above `HudField` in `crates/nl-wasm/src/lib.rs`. `EventKind` 15 and 16 exist since G2 (`Scrape`: `[15, normal_speed_m_s, 0, 0, 0]`; `TooSlowFallback`: `[16, piece_index, entry_speed_m_s, 0, 0]`).

## HudField (the array from `Game::hud`)
Today the last field in use is 27 `PolicyOutOfEnvelope` (G3); 24 `StuntPhase` and 25 `StuntCode` are in use since G4, and 26 `ApproachHint` is in use since the wrap-up, so the array has 28 values.

| Number | Name | Meaning | Added by |
|---|---|---|---|
| 23 | `PeakGLoad` | Raw peak g of the current stunt window, in g (see `g-meter.md`) | A1 `g-peak-sim` |
| 24 | `StuntPhase` | The stunt phase, 0 to 8 (see `stunt-states.md`) | G4 |
| 25 | `StuntCode` | Which stunt: 1 barrel left, 2 barrel right, 3 front flip, 4 back flip, 0 none | G4 |
| 26 | `ApproachHint` | The entry speed in m/s that the piece ahead needs (`v_complete_min_m_s`) while the car is driving up to it too slowly, else 0; the HUD says "Too slow to clear this piece: need N km/h". "Ahead": before its entry line by at most 80 m, within half its width + 2 m, nose within 60° of its axis, no record active; the nearest wins. Read-only, so it changes no run | G2 / G5 |
| 27 | `PolicyOutOfEnvelope` | 1 when the AI is outside where it was validated (speed > 55 m/s or another observation clamp), so the Hybrid uses PD | G3 (in use; set under any controller, computed by `nl_control::PolicyEnvelope`, 0 while the wreck is frozen; the limit is `[hybrid] policy_max_speed_m_s`) |

## EventKind (the first float of each event from `Game::drain_events`)
Today the last kind is 10 `HardLanding`.

| Number | Name | Meaning |
|---|---|---|
| 11 | `StuntStart` | An authorized stunt began SpinUp: `[11, code, 0, 0, 0]` |
| 12 | `StuntLanded` | The stunt reached Settled with full rotation: `[12, code, peak_g, rotation_rad, chain]` (`code` is the first stunt of the jump, `chain` packs every stunt code with the first in the ones digit: 44 is a double back flip, 24 a back flip then a barrel roll right; 0 from an older build means the single stunt `code`) |
| 13 | `StuntRefused` | The planner said no to a request: `[13, code, reason, 0, 0]` |
| 14 | `StuntAborted` | The planner stopped a stunt mid-air and handed over to PD: `[14, code, reason, rotation_rad, chain]` |

Stunt reasons (float 2 of 13 and 14): 0 not enough air (tank or start state), 1 landing blocked, 2 request expired, 3 control lost, 4 not enough time. `StuntStart` is sent once for every stunt taken into the jump, `StuntRefused` once for every one turned down, so each key press gets exactly one answer.
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
