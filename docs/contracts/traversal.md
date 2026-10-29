# Contract: approach is separate from outcome (traversal)

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.2). Built in G2 (`claude/fearless-pieces`).

## What it is
A record kept while the car goes over one piece. It answers "did the car come in cleanly?", separately from "what happened next?".

It will live in a new file, `crates/nl-sim/src/traversal.rs`:

```text
PieceTraversal { piece_id, entry_tick, entry_speed_m_s, approach: Approach, phase: Entering | Traversing | Airborne | Exited }
Approach { clean: bool, heading_err_rad, lateral_offset_m, pitch_rad, roll_rad, wheels_down, reason }
```

## When an entry is clean
Judged in the **piece's local frame**, at the moment the car enters. All limits come from `configs/approach.toml` (stub added in Gate 0, not read by any code yet).

| Check | Limit *(start)* | `approach.toml` key |
|---|---|---|
| Heading error against the piece's forward axis | ≤ 15° | `max_heading_err_deg` |
| Lateral offset from the piece centre line | inside the piece width minus 0.5 m | `lateral_margin_m` |
| Pitch relative to the piece surface | ≤ 10° | `max_pitch_deg` |
| Roll | ≤ 10° | `max_roll_deg` |
| Wheels on the ground | ≥ 3 (ground-entry pieces only) | `min_wheels_down` |

Flip plates entered while airborne are exempt from the wheel check. If an entry fails a check, `clean` is false and `reason` names the first check that failed.

## When the record ends
Whichever comes first:
- the car exits the piece;
- a timeout: piece length / entry speed + **3 s** (`timeout_extra_s`);
- contact with a collider that does not belong to the piece.

## What it must never do
- **The traversal never makes the car immune.** Crash classification always uses the real contact geometry (see `crash-scrape.md`).
- It only feeds three things: the HUD hint, test assertions and the survival metric (`survival.md`).

## Tests that pin this (plan §3)
- **Approach boundaries:** just inside and just outside each limit (heading, lateral, pitch, roll), `clean` flips as expected.
- Outside the limits, crashes happen where the geometry says they should (no immunity, no false crashes).
- **Survival:** the 0-crash sweep in `survival.md` counts only clean entries.
