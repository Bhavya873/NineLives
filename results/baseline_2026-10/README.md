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

Not here yet: the viewer landing rates.
