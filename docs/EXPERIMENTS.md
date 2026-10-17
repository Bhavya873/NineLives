# Experiments

Every training run or training chunk gets a row. Watch a replay before writing the notes; reward hacking shows up in behaviour first.

| Date | Run id | Config | Seed | Steps (total) | Wall time | Success per stage | Notes |
|---|---|---|---|---|---|---|---|

## Baseline evals (no training)

Rust eval runner (`cargo run --release -p nl-control --example eval`), 1000 episodes per row, seeds 0–999, no domain randomization, `configs/*.toml` as committed. Full per-episode rows in `results/eval_<controller>_<stage>_<mode>.json`. Success = a save, then 1.0 s with no crash (SPEC §9.5); episodes cut at 8 s.

### 2026-10-17 — M4: Off vs PD on Stages A and B

| Controller | Stage | Mode | Success | Save | Crash | Air per save (kg) | Touchdown spin median (rad/s) | Touchdown fall median (m/s) | Touchdown fall p95 (m/s) |
|---|---|---|---|---|---|---|---|---|---|
| off | A | realistic | 9.5% | 9.5% | 87.6% | 0.00 | 2.44 | 4.45 | 10.67 |
| off | A | arcade | 9.5% | 9.5% | 87.6% | 0.00 | 2.44 | 4.45 | 10.67 |
| off | B | realistic | 14.6% | 14.6% | 8.7% | 0.00 | 0.72 | 0.19 | 0.31 |
| off | B | arcade | 14.6% | 14.6% | 8.7% | 0.00 | 0.72 | 0.19 | 0.31 |
| pd | A | realistic | 69.4% | 69.4% | 29.5% | 50.22 | 0.60 | 4.04 | 6.42 |
| pd | A | arcade | 72.9% | 72.9% | 26.0% | 55.45 | 0.57 | 4.05 | 6.38 |
| pd | B | realistic | 100.0% | 100.0% | 0.0% | 36.29 | 1.25 | 0.34 | 0.52 |
| pd | B | arcade | 100.0% | 100.0% | 0.0% | 34.29 | 1.25 | 0.34 | 0.55 |

Notes:
- **PD saves far more than Off on Stage A** (69.4% vs 9.5% in Realistic): the M4 acceptance check. PD also cuts the p95 touchdown fall speed from 10.7 to 6.4 m/s and the median touchdown spin from 2.4 to 0.6 rad/s.
- PD's remaining Stage A crashes in Realistic: 236 hard impacts, 58 upside-down landings and 1 stuck; the median peak tilt in those episodes is 134°. Big starting tilts in short falls leave PD too little time to right the car and brake. This is the gap the AI should close.
- Off is identical in both modes (it never fires). PD is almost identical too: it plans with the mode's thrust scale, so Arcade's doubled thrust means half the throttle for the same push; Arcade only helps where Realistic thrust saturates.
- **Stage B needed one tuning step.** With the SPEC's plain PD on the ground, PD succeeded only 22% of the time (most episodes timed out lying on the side, using ~520 kg of air per save). Tipping a 1.8 t car over its side edge needs ~18 kN·m, more than I·kp·error gives. A `ground_roll_gain` multiplier on the roll torque while lying on the ground (`configs/controllers.toml`) fixed it: Stage B success by gain (100 episodes): 1 → 22%, 3 → 70%, **5 → 100%**, 8 → 100%. Stage A was unaffected (65–67% on that smaller sample). Committed at 5.
- An earlier 200-episode run (before scenario draws got their own random stream, so with different starts) gave PD 75.5% on Stage A Realistic; at 1000 episodes the rate settles at 69%. Treat ±3 points as noise at 200 episodes.

