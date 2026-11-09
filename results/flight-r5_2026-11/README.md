# flight-r5_2026-11

Evaluation of the r5 policy (`models/policy.json`, run `flight-r5`, 60M-step "best" checkpoint) against the policy it replaced (R4, run `flight-r4`), and of the rejected r6 retry. The decision and the reasons are in `docs/DECISIONS.md` (2026-11-09). The training runs are logged in `docs/EXPERIMENTS.md`.

## What the files are

Each `seeds_<first seed>/` folder is one block of 1000 episodes per cell, seeds `first seed` to `first seed + 999`, run with the Rust eval (`cargo run --release -p nl-control --example eval`), Hybrid with the 120 degree hand-over, Stages A to F, both modes, on the code of `claude/night` before the promotion. Files:

- `old.json`: the R4 policy (`policy_hash` 40d054618d409972), the shipped policy before this change.
- `r5.json`: the r5 best checkpoint (`policy_hash` 5c325d425e7b3f90), the shipped policy now.
- `r6.json`: the r6 best checkpoint (`policy_hash` d36861f5a7859fb7), rejected. Only in `seeds_4000000/`.

Inside a file:

- `widened` is the default draws (`configs/scenarios.toml` `[widening]` on). `legacy` is the same cells with `--legacy` (the Stage A and F draws every result before November 2026 used).
- Cell names are `<controller>_<stage>_<mode>`; `realistic` is Limited air and `arcade` is Unlimited air. `pd_*` cells are the tuned PD alone (identical for every policy; kept so the Hybrid can be read against it).
- `summary` keeps ten fields of the full eval summary. In widened Hybrid cells of Stages A and F, `by_preset` and `by_axis` keep the landing and crash rate per flip plate preset and per spin axis (`roll_flip` is the mixed plate).
- `widened_meta` and `legacy_meta` record the seeds, the hashes and the git sha of the runs.

The full per-episode eval files (about 11 MB per cell set) are not committed, as for `results/widening_2026-11/`. Every number here can be rerun with the eval command above, `--seed <first seed>`, `--policy <file>`; one cell was rerun on the promotion branch (r5, Stage F, Limited air, seeds 4,000,000+) and matched the earlier run episode for episode.

## Blocks

| Folder | Role |
|---|---|
| `seeds_1500000/` | The validation block of the retrain, where the original gate was applied and failed. `old` and `r5` only. |
| `seeds_3000000/` | First fresh block for the revised rule. This was the config's reserved final-test block (3,000,000 to 3,000,999); it is now used and no longer untouched. |
| `seeds_4000000/` | Second fresh block, never used before. `old`, `r5` and `r6`. |

## The rule

`check_gate.py` recomputes the four conditions of the revised rule from these files; `gate.txt` is its output.

| Block | Widened 12-cell mean, old to r5 | Worst widened cell | Stage F limited air, old to r5 | Revised rule |
|---|---|---|---|---|
| 1,500,000+ | 91.30 to 91.68 | -1.3 (F Unlimited) | 61.3 to 67.4 | pass |
| 3,000,000+ | 90.82 to 91.44 | -0.8 (F Unlimited) | 59.9 to 67.2 | pass |
| 4,000,000+ | 90.54 to 91.28 | -0.4 (F Unlimited) | 58.1 to 66.0 | pass |
| 4,000,000+, r6 | 90.54 to 91.29 | -3.9 (F Unlimited) | 58.1 to 68.3 | fail (condition 3) |

Known weak case: the mixed `roll_flip` plate. Landing rate on Stage F, r5, by block: 34.4, 32.7 and 29.5 percent in Limited air and 32.6, 37.3 and 38.6 percent in Unlimited air (old policy on the same blocks: 30.8, 24.9, 22.4 and 34.4, 38.7, 39.0). It did not get good with r5 or r6.
