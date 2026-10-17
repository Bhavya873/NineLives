# Experiments

Every training run or training chunk gets a row. Watch a replay before writing the notes; reward hacking shows up in behaviour first.

| Date | Run id | Config | Seed | Steps (total) | Wall time | Success per stage | Notes |
|---|---|---|---|---|---|---|---|
| 2026-10-17 | stage-a-001 | `configs/train/base.yaml` (30M steps, randomization on), reward as SPEC §9.4 | 0 | 30.0M | 17.8 min (28k steps/s, M1 Pro) | A: best fixed-seed eval 74.5% (hardest level, Realistic, no DR, 200 seeds) at 28M; level 4 reached at ~7.5M | Watched 20 episodes: no hovering (0.8–1.6 s of flight, then a firm landing). Crashes are hard body impacts in the first 0.1–0.8 s from steep, low starts. **Defect:** after landing the policy kept S_RL at full throttle (plus some lift and boost) while sitting on its wheels; the 1 s success hold hides the cost, but in the game it would drain the tank while driving. Fixed with a ground-thrust cost (DECISIONS); policy not kept. |
| 2026-10-17 | stage-a-002 | base.yaml at 50M steps; reward + ground thrust −0.02 × Σu while ≥ 3 wheels touch | 0 | 50.0M | 29.2 min | A: best eval 63.0% at 42M; level 4 only at ~31M | Idle thrust fixed (throttle sum on the ground 0.1–0.3, was ~1.5), but learning stalled at level 0 for 13M steps: timeouts with the car down and still firing, never completing the 4-wheel hold. Charging from 3 wheels taxes the thrust that settles the last wheel. New failures: upside-down crashes after long flights that empty the tank. Narrowed to all four wheels at weight 0.01 for stage-a-003. Policy not kept. |
| 2026-10-17 | stage-a-003 | base.yaml at 50M; ground thrust −0.01 × Σu only while all 4 wheels touch | 0 | 50.0M | 29.9 min | A: best eval 61.5%; level 4 only at ~47M | **Reward hack:** the 10M checkpoint lands, then holds L_RL at 0.6 for the rest of the episode to keep one wheel up. Three wheels down means no ground-thrust charge, and a timeout costs nothing (SB3 bootstraps truncation). A charge that depends on contact state teaches the policy to dodge the state, not to close the valves. Term removed; air made 4× dearer everywhere instead (stage-a-004). Policy not kept. |
| 2026-10-17 | **stage-a-004** | base.yaml at 50M (randomization on); air weights 4× (limited 8.0, unlimited 0.008) | 0 | 50.0M | 29.0 min | A: best eval 71.0% at 48M (exported); level 4 at ~7M | **Kept → `models/policy.json`.** Watched 20 episodes: 0.8–2.0 s of flight then a firm landing, no hovering, no lifted wheel; throttle sum at rest 0.2–0.4 (run 001: ~1.5), about what the 5 kg/s ground refill replaces in Realistic. Air per save ≈ PD's. Crashes are hard body impacts in the first second from steep, low starts. Fixed-seed eval below. |

## M5 evals: Off vs PD vs AI on Stage A

2026-10-17. `python -m ninelives.eval` (episodes run in Rust, same code as the Rust runner), 1000 episodes per row, seeds 0–999, hardest Stage A level, policy `models/policy.json` (run stage-a-004 at 48M steps). DR = domain randomization (`--dr`), off unless stated. Full rows in `results/eval_<controller>_A_<mode>[_dr].json|csv`.

| Controller | Mode | DR | Success | Save | Crash | Air per save (kg) | Touchdown spin median (rad/s) | Touchdown fall median (m/s) | Touchdown fall p95 (m/s) |
|---|---|---|---|---|---|---|---|---|---|
| off | realistic | no | 6.6% | 6.5% | 90.5% | – | 3.36 | 6.41 | 10.98 |
| pd | realistic | no | 58.0% | 57.9% | 41.7% | 36.21 | 1.38 | 5.00 | 10.37 |
| **ai** | realistic | no | **74.2%** | 74.1% | 25.7% | **30.72** | 1.22 | 5.60 | 8.82 |
| off | arcade | no | 6.6% | 6.5% | 90.5% | – | 3.36 | 6.41 | 10.98 |
| pd | arcade | no | 60.6% | 60.5% | 39.1% | 42.17 | 1.31 | 4.86 | 10.47 |
| **ai** | arcade | no | **84.5%** | 84.1% | 15.5% | 32.47 | 1.16 | 5.13 | 8.36 |
| pd | realistic | yes | 57.2% | 57.1% | 42.5% | 40.55 | 1.39 | 5.09 | 10.45 |
| ai | realistic | yes | 78.8% | 78.4% | 21.2% | 35.27 | 1.09 | 5.24 | 8.34 |

Notes:
- **AI beats PD on both save rate and air per save** (Realistic: 74.2% vs 58.0%, 30.7 vs 36.2 kg), meeting SPEC §10's "AI beats PD" target. The starting targets of ≥ 80% (Realistic) and ≥ 95% (Arcade) Stage A are **not met** (74.2%, 84.5%).
- The AI does better with randomization on (78.8%) than without: it trained only on randomized cars, and the randomized episodes average more thrust (scale 1.0–2.0) and are 40% unlimited air.
- What limits success: about a quarter of Realistic starts tilt steeply with a corner near the ground; the hard impact comes within the first second, before any controller can right the car. Off and PD lose these too. Next levers (M7 or later): a longer run or a second seed (one seed per config so far, so run-to-run spread is unmeasured), and the touchdown fall speed (median 5.6 m/s): the reward pays for soft landings only through the save quality.
- **Known limit, follow-up for M7:** at rest the policy still blows a throttle sum of 0.2–0.4. In Realistic that roughly matches the 5 kg/s ground refill, so the tank stops refilling after a save; with unlimited air it keeps a faint push going. The cause is the episode rule: success ends the episode 1 s after landing, so behaviour on the ground is barely trained. Training past success (a longer still hold, or not ending at success) is the deeper fix; it changes SPEC §9.5, so it waits for the M7 curriculum work.
- Throughput: about 28k env-steps/s end to end with 64 envs (PLAN §5.2 estimated 5–20k); a 50M-step run takes about 29 min.

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

