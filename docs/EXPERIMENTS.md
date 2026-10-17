# Experiments

Every training run or training chunk gets a row. Watch a replay before writing the notes; reward hacking shows up in behaviour first.

| Date | Run id | Config | Seed | Steps (total) | Wall time | Success per stage | Notes |
|---|---|---|---|---|---|---|---|

## Baseline evals (no training)

Rust eval runner (`cargo run --release -p nl-control --example eval`), 200 episodes per row, seeds 0–199, no domain randomization, `configs/*.toml` as committed. Full per-episode rows in `results/eval_<controller>_<stage>_<mode>.json`. Success = a save, then 1.0 s with no crash (SPEC §9.5); episodes cut at 8 s.

### 2026-10-17 — M4: Off vs PD on Stages A and B

| Controller | Stage | Mode | Success | Save | Crash | Air per save (kg) | Touchdown spin median (rad/s) | Touchdown fall median (m/s) | Touchdown fall p95 (m/s) |
|---|---|---|---|---|---|---|---|---|---|
| off | A | realistic | 9.0% | 9.0% | 88.5% | 0.00 | 2.06 | 4.38 | 9.49 |
| off | A | arcade | 9.0% | 9.0% | 88.5% | 0.00 | 2.06 | 4.38 | 9.49 |
| off | B | realistic | 18.5% | 18.5% | 5.5% | 0.00 | 0.73 | 0.20 | 0.35 |
| off | B | arcade | 18.5% | 18.5% | 5.5% | 0.00 | 0.73 | 0.20 | 0.35 |
| pd | A | realistic | 75.5% | 75.5% | 22.5% | 49.72 | 0.59 | 4.03 | 6.39 |
| pd | A | arcade | 80.0% | 80.0% | 18.0% | 54.82 | 0.53 | 4.04 | 6.49 |
| pd | B | realistic | 100.0% | 100.0% | 0.0% | 34.73 | 1.25 | 0.34 | 0.49 |
| pd | B | arcade | 100.0% | 100.0% | 0.0% | 32.93 | 1.25 | 0.34 | 0.51 |

Notes:
- **PD saves far more than Off on Stage A** (75.5% vs 9.0% in Realistic): the M4 acceptance check. PD also cuts the p95 touchdown fall speed from 9.5 to 6.4 m/s and the median touchdown spin from 2.1 to 0.6 rad/s.
- PD's remaining Stage A crashes in Realistic: 39 hard impacts and 6 upside-down landings; the median peak tilt in those episodes is 129°. Big starting tilts in short falls leave PD too little time to right the car and brake. This is the gap the AI should close.
- Off is identical in both modes (it never fires). PD is almost identical too: it plans with the mode's thrust scale, so Arcade's doubled thrust means half the throttle for the same push; Arcade only helps where Realistic thrust saturates.
- **Stage B needed one tuning step.** With the SPEC's plain PD on the ground, PD succeeded only 22% of the time (most episodes timed out lying on the side, using ~520 kg of air per save). Tipping a 1.8 t car over its side edge needs ~18 kN·m, more than I·kp·error gives. A `ground_roll_gain` multiplier on the roll torque while lying on the ground (`configs/controllers.toml`) fixed it: Stage B success by gain (100 episodes): 1 → 22%, 3 → 70%, **5 → 100%**, 8 → 100%. Stage A was unaffected (65–67% on that smaller sample). Committed at 5.

