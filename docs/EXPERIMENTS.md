# Experiments

Every training run or training chunk gets a row. Watch a replay before writing the notes; reward hacking shows up in behaviour first.

| Date | Run id | Config | Seed | Steps (total) | Wall time | Success per stage | Notes |
|---|---|---|---|---|---|---|---|
| 2026-10-17 | stage-a-001 | `configs/train/base.yaml` (30M steps, randomization on), reward as SPEC §9.4 | 0 | 30.0M | 17.8 min (28k steps/s, M1 Pro) | A: best fixed-seed eval 74.5% (hardest level, Realistic, no DR, 200 seeds) at 28M; level 4 reached at ~7.5M | Watched 20 episodes: no hovering (0.8–1.6 s of flight, then a firm landing). Crashes are hard body impacts in the first 0.1–0.8 s from steep, low starts. **Defect:** after landing the policy kept S_RL at full throttle (plus some lift and boost) while sitting on its wheels; the 1 s success hold hides the cost, but in the game it would drain the tank while driving. Fixed with a ground-thrust cost (DECISIONS); policy not kept. |
| 2026-10-17 | stage-a-002 | base.yaml at 50M steps; reward + ground thrust −0.02 × Σu while ≥ 3 wheels touch | 0 | 50.0M | 29.2 min | A: best eval 63.0% at 42M; level 4 only at ~31M | Idle thrust fixed (throttle sum on the ground 0.1–0.3, was ~1.5), but learning stalled at level 0 for 13M steps: timeouts with the car down and still firing, never completing the 4-wheel hold. Charging from 3 wheels taxes the thrust that settles the last wheel. New failures: upside-down crashes after long flights that empty the tank. Narrowed to all four wheels at weight 0.01 for stage-a-003. Policy not kept. |
| 2026-10-17 | stage-a-003 | base.yaml at 50M; ground thrust −0.01 × Σu only while all 4 wheels touch | 0 | 50.0M | 29.9 min | A: best eval 61.5%; level 4 only at ~47M | **Reward hack:** the 10M checkpoint lands, then holds L_RL at 0.6 for the rest of the episode to keep one wheel up. Three wheels down means no ground-thrust charge, and a timeout costs nothing (SB3 bootstraps truncation). A charge that depends on contact state teaches the policy to dodge the state, not to close the valves. Term removed; air made 4× dearer everywhere instead (stage-a-004). Policy not kept. |
| 2026-10-17 | **stage-a-004** | base.yaml at 50M (randomization on); air weights 4× (limited 8.0, unlimited 0.008) | 0 | 50.0M | 29.0 min | A: best eval 71.0% at 48M (exported); level 4 at ~7M | **Kept → `models/policy.json`.** Watched 20 episodes: 0.8–2.0 s of flight then a firm landing, no hovering, no lifted wheel; throttle sum at rest 0.2–0.4 (run 001: ~1.5), about what the 5 kg/s ground refill replaces in Realistic. Air per save ≈ PD's. Crashes are hard body impacts in the first second from steep, low starts. Fixed-seed eval below. |
| 2026-10-17 | m7-smoke-001 | base.yaml (stages A–E, weights A .25 B .15 C .2 D .3 E .1) at 100k steps; curriculum window 50, bar 0.3; checkpoint every 50k, 20 eval episodes per stage | 0 | 131k | 11 s | all stages ≈ 0% (untrained); stayed at A level 0 | Smoke test of the M7 multi-stage harness only: trains, evaluates all five stages, saves `latest/`. No unlock: an untrained policy never passes 30%. Not kept. |
| 2026-10-17 | m7-smoke-002 | as m7-smoke-001 but bar −1 (every full window passes) | 0 | 131k | 9 s | eval mean 1% (untrained) | Exercises the curriculum: A rose through levels 1–4, then B, C, D and E unlocked in order and the mix reached all five stages (Rust picks each episode's stage). Not kept. |

## M5 evals: Off vs PD vs AI on Stage A

2026-10-17. `python -m ninelives.eval` (episodes run in Rust, same code as the Rust runner), 1000 episodes per row, seeds 0–999, hardest Stage A level, policy `models/policy.json` (run stage-a-004 at 48M steps). DR = domain randomization (`--dr`), off unless stated. Full rows in `results/eval_<controller>_A_<mode>[_dr].json|csv`.

| Controller | Mode | DR | Success | Save | Crash | Air per save (kg) | Touchdown spin median (rad/s) | Touchdown fall median (m/s) | Touchdown fall p95 (m/s) |
|---|---|---|---|---|---|---|---|---|---|
| off | realistic | no | 6.6% | 6.5% | 90.5% | – | 3.36 | 6.41 | 10.98 |
| pd | realistic | no | 58.0% | 57.9% | 41.7% | 36.21 | 1.37 | 5.01 | 10.49 |
| **ai** | realistic | no | **74.2%** | 74.1% | 25.7% | **30.66** | 1.22 | 5.60 | 8.82 |
| off | arcade | no | 6.6% | 6.5% | 90.5% | – | 3.36 | 6.41 | 10.98 |
| pd | arcade | no | 60.6% | 60.5% | 39.1% | 42.17 | 1.30 | 4.86 | 10.47 |
| **ai** | arcade | no | **84.5%** | 84.1% | 15.5% | 32.47 | 1.16 | 5.13 | 8.36 |
| pd | realistic | yes | 57.2% | 57.1% | 42.5% | 40.55 | 1.39 | 5.09 | 10.45 |
| ai | realistic | yes | 78.8% | 78.4% | 21.2% | 35.27 | 1.09 | 5.24 | 8.34 |

Notes:
- **AI beats PD on both save rate and air per save** (Realistic: 74.2% vs 58.0%, 30.7 vs 36.2 kg; numbers from the libm build, see DECISIONS), meeting SPEC §10's "AI beats PD" target. The starting targets of ≥ 80% (Realistic) and ≥ 95% (Arcade) Stage A are **not met** (74.2%, 84.5%).
- The AI does better with randomization on (78.8%) than without: it trained only on randomized cars, and the randomized episodes average more thrust (scale 1.0–2.0) and are 40% unlimited air.
- What limits success: about a quarter of Realistic starts tilt steeply with a corner near the ground; the hard impact comes within the first second, before any controller can right the car. Off and PD lose these too. Next levers (M7 or later): a longer run or a second seed (one seed per config so far, so run-to-run spread is unmeasured), and the touchdown fall speed (median 5.6 m/s): the reward pays for soft landings only through the save quality.
- **Known limit, follow-up for M7:** at rest the policy still blows a throttle sum of 0.2–0.4. In Realistic that roughly matches the 5 kg/s ground refill, so the tank stops refilling after a save; with unlimited air it keeps a faint push going. The cause is the episode rule: success ends the episode 1 s after landing, so behaviour on the ground is barely trained. Training past success (a longer still hold, or not ending at success) is the deeper fix; it changes SPEC §9.5, so it waits for the M7 curriculum work.
- Throughput: about 28k env-steps/s end to end with 64 envs (PLAN §5.2 estimated 5–20k); a 50M-step run takes about 29 min.

## M7 landing-quality rules: eval only, no training (2026-10-17)

`python -m ninelives.eval` (episodes run in Rust), 1000 episodes per row, seeds 0–999, same policy (`models/policy.json`, stage-a-004) and configs as above plus the PLAN §3.3 rules: success now also needs the commanded throttle sum < 0.05 for the last 0.5 s of the 1.0 s hold. No training was run. Before = the committed results above.

| Controller | Stage | Mode | DR | Success before → after | Save | Crash | Hard-landing rate | Settle time median (s) | Air left at success median (kg, limited tank only) |
|---|---|---|---|---|---|---|---|---|---|
| off | A | realistic | no | 6.6% → 6.6% | 6.5% | 90.5% | 0.0% | 0.74 | 260.0 |
| off | B | realistic | no | 14.6% → 14.6% | 14.6% | 8.6% | 0.0% | 1.45 | 260.0 |
| pd | A | realistic | no | 58.0% → 58.0% | 57.9% | 41.7% | 0.2% | 0.20 | 227.4 |
| pd | A | arcade | no | 60.6% → 60.6% | 60.5% | 39.1% | 0.2% | 0.18 | – |
| pd | A | realistic | yes | 57.2% → 57.2% | 57.1% | 42.5% | 0.2% | 0.23 | 208.5 |
| pd | B | realistic | no | 100.0% → 100.0% | 100.0% | 0.0% | 0.0% | 1.48 | 229.7 |
| pd | B | arcade | no | 100.0% → 100.0% | 100.0% | 0.0% | 0.0% | 1.48 | – |
| ai | A | realistic | no | **74.2% → 0.0%** | 74.1% | 25.7% | 2.1% | 0.08 | – |
| ai | A | arcade | no | **84.5% → 3.2%** | 84.4% | 15.5% | 1.7% | 0.11 | – |
| ai | A | realistic | yes | **78.8% → 0.3%** | 78.6% | 21.2% | 0.8% | 0.09 | – |

(Off rows are identical in Arcade, except that air left is empty there: Arcade has unlimited air, and so do 40% of DR episodes. Heading error and speed kept are empty on Stage A/B: both are judged only after a take-off from the wheels, which drops and tip-overs don't have. The one exception is a single AI Realistic episode that bounced off its wheels before its save.)

Notes:
- **The AI's drop is the known idle-thrust defect, not a regression in flying:** its save and crash rates are unchanged, but it keeps a throttle sum of 0.2–0.4 while parked, so the episodes now time out instead of succeeding. The next training run (with the new success rule and reward terms) has to learn to close the valves. Off and PD close their valves at rest, so their numbers are unchanged.
- The heading and speed-kept factors of q apply only after a take-off from the wheels (Stage C/D jumps), so Stage A/B save rewards change only through the new hard-landing, unsettled and air-left terms.
- Hard landings are rare on Stage A (≤ 2%): most landings that would bottom out a spring hit the body first and crash.

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


### 2026-10-17 — M7: PD with the driver bot on Stages C, D and E

`cargo run --release -p nl-control --example eval -- --controller pd --stage {C,D,E} --mode realistic --episodes 100 --seed 0` (no randomization; files not committed, the full eval matrix comes with the M7 training run).

| Controller | Stage | Mode | Success | Crash |
|---|---|---|---|---|
| pd | C | realistic | 100% | 0% |
| pd | C | arcade | 100% | 0% |
| pd | D | realistic | 98% | 1% |
| pd | D | arcade | 98% | 1% |
| pd | E | realistic | 82% | 17% |
| pd | E | arcade | 79% | 20% |

Notes:
- Stage D needed three bot and course fixes to get here from 75%: the bot centres its steering in the air (it landed with the wheels turned and swerved); its pursuit turn is capped at 6 m/s² sideways, with the wobble and jerks scaled to that grip-safe turn (uncapped, it spun the car whenever it landed off the line; capped but with the wobble in raw steer units, the wobble overpowered the pursuit and the car drifted off the 20 m track: 33%); kickers are 3.5–4.5 m wide (at 2 m an off-centre car put its body on the slope) and ramps 8–10 m wide. Walls are off (weight 0): 8 of the last 11 failures were hard landings 50 m past a wall at 20–30 m/s.
- Measured after rebasing onto the landing-quality rules (quiet-throttle success for A/B, HardLanding, settle time) and with the Stage C/D landing heading limit of 30°: PD's median landing heading error is 0.3° on C and 0.05° on D, so the limit costs PD nothing; it is there for the AI. Stage E's failures are mostly base-A drops, where PD loses about 40% anyway.


### 2026-10-18/19 — M7: full A→E training runs (m7-full-001…003)

`python -m ninelives.train --config configs/train/base.yaml --total-steps 150_000_000 --run-id m7-full-00N` (the runs used `configs/train/m7-full.yaml`, base.yaml with 150M steps, since replaced by the `--total-steps` flag): seed 0, 150M steps planned, all five stages via the curriculum, domain randomization on, 64 envs, ~26k steps/s (150M ≈ 95 min plus evals). Plan agreed with the owner (docs/PLAN.md §3.4): up to 3 runs, then review gate 2.

| Run | Code | Steps | Wall | Curriculum reached | Stage A full difficulty, eval (Realistic, 200 seeds) | Outcome |
|---|---|---|---|---|---|---|
| m7-full-001 | 73a644e | 5M (stopped) | 3 min | A level 0, 0% | – | Training episodes never succeed: PPO's exploration noise keeps the throttle sum above the 0.05 quiet limit, so the window-based curriculum can't move. Deterministically the same policy already landed 34% of level-0 drops quietly. |
| m7-full-002 | 84e012d | 86M (stopped) | 55 min | A level 3 | peak 52% at 13M → 8% at 85M; crash flat ~47% | Promotion now by deterministic eval, throttles start at the edge of zero. It learned to land, then drifted to idling thrust (throttle sum 0.52 deterministic; action std collapsed to 0.03): 46% of drops landed and timed out. Nothing in training paid for being quiet. |
| m7-full-003 | badc2cc | 150M | 100 min | A level 3 (never unlocked B) | best 56% at 15M; final 42%, crash 58% | Success bonus (+5) fixed the idling (timeouts ≈ 0; success ≈ 1 − crash). But level 3 eval stayed ~45–56%, under the 85% bar, for 140M steps, so B–E never unlocked. |

Smoke runs on the way (3–6M steps, not logged separately): an initial action bias of −2 sampled almost no firing and never learned to brake (level-0 eval fell from 31% to 5%); a bias of −1 (the edge of zero throttle) reached level 0 at 91% by 2M.

**Against the bar** (beat PD on every stage and mode): **not met.** The best checkpoint (56%) is level with PD on Stage A Realistic (58%), below the M5 policy under the old success rule (74%), and B–E were never trained. `models/policy.json` and the viewer checkpoints are unchanged.

Notes:
- The binding constraint is now **crashes**, not quietness: 45–60% of full-difficulty drops end in hard impacts or upside-down landings. M5's stage-a-004 crashed far less. Candidates: the new HardLanding and unsettled costs making the policy hesitant to brake late; less time at full difficulty (stuck at level 3, never level 4); a curriculum bar (85%) far above what even PD reaches at full difficulty (58%).
- The 85% promotion bar suits easy levels, but at the upper Stage A levels it blocks every later stage. Options for the next run are in the PR: a lower or per-level bar, unlocking B–E on time or on beating PD, or warm-starting from stage-a-004's braking skill.


### 2026-10-19 — M7 gate-2 round: cost ablation and warm-started run (m7-full-004)

Owner's gate-2 choices: warm start from `stage-a-004`, per-level bars [0.85, 0.85, 0.75, 0.65], unlock on matching PD or every 15M steps, a cost ablation, one full run, the bar unchanged. **Seed hygiene:** training uses random sim seeds; validation (promotion, fresh seeds each checkpoint, and best-checkpoint choice) uses seeds ≥ 1,000,000; the test seeds 0–999 were used once, for the final eval below.

**Ablation** (20M steps each, warm start, run in parallel, validation seeds):

| Run | Reward change | Validation success / crash at 20M |
|---|---|---|
| abl-base | none | 0.0% / 14.7% |
| abl-nohard | `hard_landing: 0` | 0.0% / 16.0% |
| abl-nocosts | `hard_landing: 0`, `unsettled: 0` | **81.3%** / 15.0% |

The unsettled cost kept the policy from ever settling quietly (it landed safely and timed out). The hard-landing cost made no difference, so it stays (agreed rule: drop a cost only if that cuts crashes by ≥ 5 points). `reward.toml` sets `unsettled = 0.0`.

**m7-full-004:** `python -m ninelives.train --config configs/train/base.yaml --total-steps 150_000_000 --init-from runs/stage-a-004/best --run-id m7-full-004` at commit a6164e7, seed 0, 150M steps, about 2 h (21.5k steps/s). B unlocked on time at 15M, then C, D and E. Stage A reached level 4. Validation success over A–E rose from 66% (45M) to 75.4% (best, 147M); crash fell from 29% to 20%.

**Final test eval** (seeds 0–999, 1000 episodes each, best checkpoint; AI files kept aside since the bar was not met):

| Stage | Realistic AI / PD | Arcade AI / PD |
|---|---|---|
| A | **66.3%** / 58.0% | **78.9%** / 60.6% |
| A (randomized) | **59.7%** / 57.2% | – |
| B | 100% / 100% | 99.9% / 100% |
| C | 100% / 100% | 99.8% / 100% |
| D | 64.7% / **97.3%** | 91.2% / **97.3%** |
| E | 56.9% / **83.1%** | 75.1% / **80.4%** |

- **Generalization:** test success averages 77.6% (A–E, Realistic) against 75.4% on validation, so there is no sign of overfitting to the validation seeds.
- **Against the bar:** not met. The AI beats PD on drops (Stage A, all three conditions), ties on B and C, and loses on procedural courses (D: 33% crashes in Realistic) and knocks (E). D and E only joined the mix after ~30–40M steps.
- PD's Stage A/B files came out byte-identical to the committed ones (determinism holds). PD's C/D/E results are committed as new baselines.


### 2026-10-19 — M7 rounds 2–4: fixing Stage D's off-track failures (20M-step ablations, validation seeds)

| Run (from m7-full-004 best, D/E-heavy weights) | A | B | C | D (out of bounds) | E |
|---|---|---|---|---|---|
| m7-full-004 (start) | 66.3% | 100% | 100% | 63.0% (26%) | 56.0% |
| sideways 0.02 / 0.1 | – | – | – | 30.7% / 51.3% | 34.7% / 52.3% |
| ground_thrust 0 (control) | – | – | – | 5.0% (86%) | 15.7% |
| ground_thrust 0.05 | 0% (68% timeouts) | – | 100% | 76.0% (11%) | 17.7% |
| ground_thrust 0.2 + deadband | 0% | 0% | 100% | 66.7% (8%) | 17.0% |
| ground_thrust 0.5 + deadband | 0% | 0% | 100% | 76.0% (11%) | 17.7% |

- A trace showed the AI leaves the Stage D track on the ground, always off the right edge, with its side nozzles firing ~1.0 throttle while the bot drives.
- Charging sideways speed made it worse. Charging thrust while steady on the ground fixes driving (out of bounds 26% → 8–11%), but every variant breaks drops and tip-overs (A and B go to 0%, timing out).
- No candidate met the round-4 rule (beat the start on D and E, lose ≤ 3 points on A, B and C). The 100M-step continuation was not run.
- Next to investigate: why the ground-thrust cost stops A/B landings from succeeding even with the 0.02 deadband (likely a remaining trickle, or the cost firing during the landing itself).


### 2026-10-19/20 — M7 rounds 5–6: three-approach batch, residual-100m, PD tuning

**Round 5 batch** (owner's choice): three approaches in parallel, 25M steps each, scored on validation seeds from 1,500,000 (300 episodes per cell, Stage A at level 4), A–E in both modes. Rule: most cells beating PD wins; mean success breaks ties.

| Run | Approach | Cells beating PD (of 10) | Realistic A / D / E | Arcade A / D / E |
|---|---|---|---|---|
| batch-ppo | PPO continued from m7-full-004 best, LR 1e-4, D/E-heavy weights | 2 | – | – |
| batch-bc | behaviour cloning from PD, then PPO | 2 | – | – |
| **batch-residual** | PD + 0.5 × policy action (head zeroed, so it starts as PD) | **3** | 68.3 / 95.7 / 81.7 | 74.7 / 84.3 / 76.7 |
| PD (then) | – | – | 59.7 / 95.7 / 81.3 | 60.3 / 95.7 / 79.0 |

**residual-100m:** `configs/train/residual.yaml` (worktree m7-residual, commit 87ece93), fresh, seed 0, 100M steps, all stages at their hardest level from the start (weights A .25 B .05 C .1 D .4 E .2), 2.3 h. Checkpoint evals (200 episodes per stage, seeds from 1,000,000) from TensorBoard:

| Steps | A | B | C | D | E | Mean |
|---|---|---|---|---|---|---|
| 1M | 60.5% | 100% | 100% | 97.0% | 83.0% | **88.1% (best)** |
| 21M | 62.0% | 100% | 100% | 89.0% | 80.0% | 86.2% |
| 41M | 64.0% | 100% | 100% | 87.5% | 80.0% | 86.3% |
| 61M | 61.5% | 100% | 100% | 84.5% | 78.5% | 84.9% |
| 91M | 60.5% | 100% | 100% | 84.5% | 79.0% | 84.8% |

Final checkpoint, validation (seeds from 1,000,000, 300 episodes): Realistic A 62.7, B 100, C 100, D 96.0, E 80.3; Arcade A 71.0, B 100, C 100, D 92.3, E 79.3. It beats the PD of the time on 4 of 10 cells: **bar not met**. Not exported.

- **The residual never improved on PD.** The best checkpoint is the first (1M steps, nudges still near zero). Training gained at most 3–4 points on A and gave back 12 on D and 4 on E.
- The batch's "3 cells vs 2" margin was inside 300-episode noise (±2.8 points standard error at 60%), so the batch picked a winner that didn't hold over a long run.
- Round-6 diagnosis: the nudge was not divided by `thrust_scale` while PD's allocation is, which overshot in Arcade. The fixes, **R1** (`residual-r1.yaml`, force-scaled nudge) and **R2** (`residual-r2.yaml`, force-scaled + a learned gate), were **planned, not run** (commit c08f1b4): round 7 changed the sim first, and the owner then dropped residual as the main line (DECISIONS 2026-10-21).

**PD tuning** (`python -m ninelives.tune_pd`, CMA-ES on tuning seeds ≥ 2,000,000, scored on A, D and E in both modes):

| Version | Scored on | Result | Outcome |
|---|---|---|---|
| v1 | plain car only | huge gains, burned the tank; randomized D 93% → 40% | rejected (overfit) |
| v2 | plain + randomized, fraction gains capped, pre-round-7 PD | 0.914 vs repo 0.847 on its own tuning seeds | never validated; obsolete after round 7's PD changes |
| v3, v4 | round-7 PD | see the round-7 rows | – |

**Repo PD on the round-7 sim** (untuned gains), validation seeds from 1,500,000, 300 episodes, no randomization: Realistic A 70, B 100, C 100, D 99, E 85; Arcade A 71, B 100, C 100, D 100, E 84. The M5 policy (stage-a-004) on test seeds: Stage A 81.0% Realistic, 87.8% Arcade. That passes the 80% Realistic stretch target, mostly thanks to the interlock and the chamfered underside.

Conclusion (owner, 2026-10-21): one policy owning the nozzles through 45 s of driving keeps losing to PD on D/E, while the Stage-A-only policy is the best AI. Next round: **the AI flies, PD drives** (a `Hybrid` controller), paired-seed evals with confidence intervals, and a revised bar (DECISIONS).


### 2026-10-21 — Hybrid with the M5 policy, no training (PR #44)

The `Hybrid` controller (SPEC §8) with the shipped `models/policy.json` (stage-a-004, Stage A only) against repo-gains PD on the round-7 sim. Rust eval runner, validation seeds from 1,500,000, 1000 episodes per cell, no randomization; paired on the same seeds (Wilson 95% intervals, exact McNemar test). Test seeds 0–999 not used.

**Gate v1: the AI takes over at take-off** (fewer than 3 wheels down, or tilt > 60°): **fails**. A 81.3 / 86.6 (PD 68.9 / 69.4), B within CI, but C 34.6 / 57.2 (PD 100 / 99.8: 602 runs out of bounds, median 191 kg air, the AI keeps firing after the hop), D 12.5 / 22.2 (PD 98.8 / 98.9; Off gets 24.7%: the policy has never flown an upright jump), E 45.8 / 49.1.

**Gate v2: the AI takes over only past the upset tilt (60°)**, committed:

| Stage | Realistic PD → Hybrid | Arcade PD → Hybrid | Only PD / only Hybrid succeeded (R; A) | Against the 2026-10-21 bar |
|---|---|---|---|---|
| A | 68.9 → **81.0** (p 4e-21) | 69.4 → **86.4** (p 2e-41) | 28 / 149; 10 / 180 | beats PD |
| B | 99.4 → 99.5 | 99.4 → 99.7 | 5 / 6; 3 / 6 | within PD's CI |
| C | 100 → 100 | 99.8 → 99.8 | 0 / 0; 0 / 0 | within PD's CI |
| D | 98.8 → 98.7 | 98.9 → 98.7 | 1 / 0; 2 / 0 | within PD's CI |
| E | 85.5 → **90.7** (p 6e-11) | 83.8 → **89.8** (p 6e-15) | 8 / 60; 4 / 64 | beats PD |

- **Meets the M7 bar on all ten cells on validation, without training.** Confirmed on the test seeds (below).
- Air per save is lower than PD's (A Realistic 35.2 vs 39.7 kg; Arcade 38.6 vs 44.9). Hard landings are a little higher on A (Realistic 4.4% vs 0%, Arcade 3.5% vs 2.0%); they still count as successes.
- Lesson: the take-off gate handed the policy states it never trained on. Hand a learned controller only the states it was trained for, and leave the rest to the controller that already handles them.

**Test seeds 0–999 (one-time, owner-approved), gate v2, 1000 episodes per cell; files `results/eval_hybrid_*` (PR #44), table from `python -m ninelives.compare --candidate hybrid`:**

| Stage | Realistic PD → Hybrid | Arcade PD → Hybrid | Only PD / only Hybrid succeeded (R; A) | Against the 2026-10-21 bar |
|---|---|---|---|---|
| A | 67.0 → **81.0** (p 4e-24) | 69.2 → **87.6** (p 2e-43) | 33 / 173; 13 / 197 | beats PD |
| B | 99.2 → 99.0 | 99.3 → 99.8 | 10 / 8; 2 / 7 | within PD's CI |
| C | 100 → 100 | 99.6 → 99.6 | 0 / 0; 0 / 0 | within PD's CI |
| D | 99.5 → 99.3 | 99.3 → 99.3 | 2 / 0; 0 / 0 | within PD's CI |
| E | 86.8 → **90.7** (p 2e-6) | 85.4 → **90.6** (p 2e-13) | 14 / 53; 3 / 55 | beats PD |

- **The M7 bar is met on all ten test cells.** The PD reruns matched the committed `results/eval_pd_*` files episode for episode.
- Owner decision after the result: the game defaults to Hybrid once the policy loads (DECISIONS).
### 2026-10-21 — Round 7: PD tuning v3 and v4 on the round-7 sim (CMA-ES, not adopted)
`python -m ninelives.tune_pd --generations 20 --popsize 10` (20 × 10 candidates, 200 episodes per cell, tuning seeds from 2,000,000; A, D, E × both modes × plain/randomized). Each took about 3 h on 8 cores. Validated with 300 episodes per cell on seeds from 1,500,000, against the repo gains.

| Run | What was tuned | Validation mean success | A | E | B | D | Air | Bumps strip (8 s) | Outcome |
|---|---|---|---|---|---|---|---|---|---|
| repo | — | 0.893 | 70/71 | 85/84 | 100 | 99/100 | — | 6.7 kg | bar |
| v3 | every [pd] number except the air-control feel and the accel cap | 0.938 | +15 to +18 | +5 to +11 | −3 | −1 to −3 | +50–70% | 17.4 kg | rejected: brakes every fall over 0.4 m/s (`brake_speed_m_s` 4.0 → 0.39), fires on bumps, brakes fast falls too late (`brake_safety` 0.62) |
| v4 | v3 minus the rules for when PD acts (brake speed and safety, upright alignment, quiet and lying thresholds) | 0.939 | +14 to +16 | +5 to +10 | −1 to −2 | 0 | +25–50% | 10.6 kg | not adopted (owner): still fires more over bumps; `brake_usable_fraction` 0.99 starts fast-fall braking lower |

- Tuning on success alone moves PD toward firing more often; the search needs the owner's behaviour rules as constraints, not just as tests afterwards.
- v5 also froze where braking starts (brake height, margin, usable fraction) and paid 0.05 success per kg past 8 kg over the bumps strip, in either mode. Validation mean 0.893 → 0.919: A +8 to +12, E +2 to +7, B and C level, D −2 (99 → 97 plain, 97 → 95 randomized), air +5–10%, bumps strip 7.9 kg; all tests pass with it.
- **Decision (owner, 2026-10-21): keep the repo gains.** Under Hybrid the AI flies the upsets, where v5's gains are, and PD drives and lands upright jumps (Stage D), where v5 is worse. Hybrid already clears the M7 bar against the repo PD. v5's file is not committed; rerun with the same command if wanted.
