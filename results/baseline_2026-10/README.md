# Baseline, October 2026 (plan G1)

PD and Hybrid on stages A to F, both modes, plain and domain-randomized, recorded under the
physics as it stood before the stunts/speed/viewer work. Kept apart from later results so a
number from the new physics is never compared to one from the old by mistake.

- 48 files, 1000 episodes each, seeds 1,500,000 and up (the validation block; the 3,000,000+ block is untouched).
- Every file carries `physics_hash` (`db310175722eef10`), `controller_config_hash`
  (`dd24c53fe5654c16`), `policy_hash` (Hybrid: `40d054618d409972`, the R4 policy; PD: null)
  and `git_sha` (`acb28fc4…`). A result from different settings has a different hash.
- Means over the 12 cells (stage × mode): PD 94.84% plain / 88.74% randomized; Hybrid
  (hand-over 120°) 94.93% / 92.14%. These match the R4 validation in `docs/EXPERIMENTS.md`.

Regenerate one file (from the repo root; `--dr` for randomized):

    cargo run --release -p nl-control --example eval -- --controller pd --stage A \
      --mode realistic --episodes 1000 --seed 1500000 --out results/baseline_2026-10

`speed_sweep.json` and `.csv`: the per-piece speed sweep under the same physics (16,320 runs, 1295 crashes), with its cause table in `docs/research/speed-survival.md`.

`viewer_landing_rates.json`: the **viewer landing rates**. Each of the 8 shipped viewer checkpoints
(`web/public/checkpoints/flight-r4`, 2M to 90M steps) flies 64 Realistic cars (seeds 0 to 63, crowds of 8
as the viewer shows them) on each stage the viewer covers, through the same wasm `Crowd` (the Hybrid, 120°
hand-over, the checkpoint as its AI), counted as landed / crashed / still flying. Stage A flat: 72% landed at
2M steps, 78-86% from 16M on (checkpoint 3 best, 86%). Arena (obstacles pulled into the drop zone): 41% at
2M, 52-58% after (6-7 cars end stuck on a piece). Stage D: 98% at every checkpoint (PD drives, the AI never
takes over). Stage F: 3% at 2M, 48% at 16M, 83% at 88M and 80% at 90M. Gauntlet: 83% at every checkpoint
(PD again). The file records `physics_hash`, `controller_config_hash`, each checkpoint's `policy_hash` and
`git_sha` like the eval files. Regenerate it (from the repo root):

    cargo run --release -p nl-wasm --example viewer_rates -- \
      --checkpoints web/public/checkpoints --out results/baseline_2026-10/viewer_landing_rates.json
