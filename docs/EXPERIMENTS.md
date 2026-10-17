# Experiments

Every training run or training chunk gets a row. Watch a replay before writing the notes; reward hacking shows up in behaviour first.

| Date | Run id | Config | Seed | Steps (total) | Wall time | Success per stage | Notes |
|---|---|---|---|---|---|---|---|

## Baseline evals (no training)

Rust eval runner (`cargo run --release -p nl-control --example eval`), 1000 episodes per row, seeds 0–999, no domain randomization, `configs/*.toml` as committed. Full per-episode rows in `results/eval_<controller>_<stage>_<mode>.json`. Success = all four wheels down for 1.0 s unbroken, with no crash (SPEC §9.5); episodes cut at 8 s. Touchdown = the first step anything (wheels, body or roof) touches the ground. Air per save = air used during saved upsets ÷ saves.

### 2026-10-17 — M4: Off vs PD on Stages A and B

| Controller | Stage | Mode | Success | Save | Crash | Air per save (kg) | Touchdown spin median (rad/s) | Touchdown fall median (m/s) | Touchdown fall p95 (m/s) |
|---|---|---|---|---|---|---|---|---|---|
| off | A | realistic | 6.6% | 6.5% | 90.5% | 0.00 | 3.36 | 6.41 | 10.98 |
| off | A | arcade | 6.6% | 6.5% | 90.5% | 0.00 | 3.36 | 6.41 | 10.98 |
| off | B | realistic | 14.6% | 14.6% | 8.6% | 0.00 | 0.27 | 0.69 | 0.98 |
| off | B | arcade | 14.6% | 14.6% | 8.6% | 0.00 | 0.27 | 0.69 | 0.98 |
| pd | A | realistic | 58.0% | 57.9% | 41.7% | 36.21 | 1.38 | 5.00 | 10.37 |
| pd | A | arcade | 60.6% | 60.5% | 39.1% | 42.17 | 1.31 | 4.86 | 10.47 |
| pd | B | realistic | 100.0% | 100.0% | 0.0% | 35.93 | 0.50 | 0.77 | 1.13 |
| pd | B | arcade | 100.0% | 100.0% | 0.0% | 33.06 | 0.50 | 0.77 | 1.16 |

Notes:
- **PD saves far more than Off on Stage A** (58.0% vs 6.6% in Realistic): the M4 acceptance check. PD also cuts the median touchdown spin from 3.4 to 1.4 rad/s and the median touchdown fall speed from 6.4 to 5.0 m/s.
- PD's Stage A crashes in Realistic: 329 hard impacts and 88 upside-down landings; the median peak tilt in those episodes is 149° (successes: 62°). Big starting tilts in short falls leave PD too little time to right the car and brake. This is the gap the AI should close.
- 6–7 episodes per Stage A row crash in under 0.1 s: a steep tilt puts a corner just 5 cm (`spawn_clearance_m`) above the ground while falling at up to 5 m/s. Those are real impacts that no controller can prevent; they cap Stage A success just below 100%.
- Off is identical in both modes (it never fires). PD is almost identical too: it plans with the mode's thrust scale, so Arcade's doubled thrust means half the throttle for the same push; Arcade only helps where Realistic thrust saturates.
- **Stage B needed one tuning step.** With the SPEC's plain PD on the ground, PD succeeded only 22% of the time (most episodes timed out lying on the side, burning air the whole time). Tipping a 1.8 t car over its side edge needs ~18 kN·m, more than I·kp·error gives. A `ground_roll_gain` multiplier on the roll torque while lying on the ground (`configs/controllers.toml`) fixed it: Stage B success by gain (100 episodes): 1 → 22%, 3 → 70%, **5 → 100%**, 8 → 100%. Stage A was unaffected (65–67% on that smaller sample). Committed at 5.
- **Earlier numbers in this section's history are not comparable.** Code review found that the first runs (PD 75.5% / 69.4% on Stage A Realistic) tilted the car about a random 3-D axis (so real tilts were often far smaller than drawn), started some tilted cars inside the ground, needed a Save event to succeed (so clean landings failed), recorded touchdowns only on wheel landings, and charged failed episodes' air to "air per save". All fixed; the table above is the baseline.

