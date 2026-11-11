# results

Every number in the README, SPEC and docs comes from a file here. Files are never edited by hand: a rerun overwrites them and git shows the change. Rust and Python write to these exact paths and the web tests read some of them, so the top-level names are fixed.

## Current eval files (top level)

Name pattern: `eval_<controller>_<stage>_<mode>.json` (+ `.csv` from the Python eval). 1000 episodes each. A `_dr` suffix means domain randomization was on.

- `eval_off_*`: no controller (Stages A and B). Cited by README and docs/EXPERIMENTS.md.
- `eval_pd_*`: the PD controller, Stages A to E, both modes (`_dr` for Stage A realistic). Cited by README, docs/EXPERIMENTS.md.
- `eval_hybrid_*`: PD plus the trained AI (r5, `policy_hash` 5c325d425e7b3f90), Stages A to E, both modes. Cited by README, docs/EXPERIMENTS.md.
- `eval_ai_*`: the AI alone on Stage A (arcade, realistic, realistic `_dr`). `eval_ai_A_realistic.json` (+ `.csv`) is read by `web/tests-node/policy.test.mjs` (wasm must match native) and was rerun with the r5 policy; the other `eval_ai_*` and the Hybrid files under `widening_2026-11/` (except `widened/eval_hybrid_A_realistic.json`) were made with the previous policy (R4, `policy_hash` 40d054618d409972) and are not regenerated. All ten `eval_hybrid_*` files were regenerated on 2026-11-10 with the shipped r5 policy (`policy_hash` 5c325d425e7b3f90, seeds 0 to 999, same physics and controller config hashes as the `eval_pd_*` files). The shipped policy's numbers on the widened draws are in `flight-r5_2026-11/`.
- `hover_hack.json`: the hover-hack experiment (SPEC 9.11, docs/TRAINING.md, README chart `docs/images/hover-hack.svg`).
- `stunt_budget.json`: air budget per stunt (docs/research/stunt-planning.md, docs/EXPERIMENTS.md).
- `speed_sweep_after_pieces.json` / `.csv`: speed sweep over all piece kinds; named in the `nl-control` survival test.
- `speed_sweep_after_floor_hook.json` / `.csv`: the same sweep after the floor contact fix, 0 crashes (docs/contracts/survival.md, docs/EXPERIMENTS.md).
- `viewer_landing_rates_after_floor_hook.json`: viewer landing rates after the floor contact fix (docs/EXPERIMENTS.md).

## Folders

- `baseline_2026-10/`: PD and Hybrid on Stages A to F, plain and randomized, under the physics before the stunts/speed/viewer work. Has its own `README.md`. Cited by SPEC (viewer rates), docs/EXPERIMENTS.md, docs/TECHNICAL.md, docs/research/speed-survival.md.
- `flight-r5_2026-11/`: the r5 promotion (DECISIONS 2026-11-09): compact per-cell Hybrid and PD results for the old policy, r5 and the rejected r6 on three seed blocks (widened and legacy draws), the viewer landing rates of the `flight-r5` checkpoints, `check_gate.py` (recomputes the promotion rule) and `gate.txt`. Has its own `README.md`. Cited by README, docs/EXPERIMENTS.md.
- `control_upgrade/`: the PD control upgrade experiments (docs/control-upgrade-audit.md, docs/EXPERIMENTS.md). Each folder has a `manifest.json` (settings, hashes, per-cell numbers).
  - `2026-10-26-legacy-pd/`: the legacy PD baseline, A to F.
  - `2026-10-26-pd-model-fix/`: the actuator-aware PD fix (manifest only).
  - `2026-10-27-pd-cma-tune/`: the CMA-ES retune that was not adopted (manifest, tuning log, validation summary).
  - `2026-10-27-gate-tilt/`, `2026-10-27-r3/`, `2026-10-27-r4/`: manifest and validation summary for the hand-over tilt test and training rounds R3 and R4.

## Removed in the repo cleanup

`widening_2026-11/before/` and `widening_2026-11/widened/`: PD and Hybrid (the policy of `models/policy.json`), Stages A and F, both modes, 1000 episodes, seed 0, made with the old draws (`--legacy`) and with scenarios.toml `[widening]`. Rows carry `preset`, `strength` and `spin_axis`; each file has `by_preset` and `by_axis` tables. Tables are in docs/EXPERIMENTS.md. `widened/eval_hybrid_A_realistic.json` is read by `web/tests-node/crowd.test.mjs` and was rerun with the r5 policy.

Raw per-candidate eval files under `control_upgrade/*/candidates/` and `control_upgrade/2026-10-26-pd-model-fix/hybrid_r2_latest/` (their numbers stay in the manifests), and the superseded speed sweeps `speed_sweep_after_drive`, `speed_sweep_after_merge` and `speed_sweep_drive_candidate` (their numbers stay in docs/EXPERIMENTS.md and docs/research/speed-survival.md). 66 files, about 12 MB.

The SVG charts in `docs/images/` are drawn from these files by `python -m ninelives.charts` (the old `report_charts` module was merged into it).
