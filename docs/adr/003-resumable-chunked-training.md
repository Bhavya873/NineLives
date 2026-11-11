# ADR-003: Resumable, chunked CPU training (checkpoints now stay in `runs/`)

**Status:** Accepted; amended 2026-10-17 (training runs locally, checkpoints stay in `runs/`, see the amendment at the end)
**Date:** 2026-10-17
**Deciders:** Bhavya Modi (owner)

## Context
Training runs in a cloud container with 4 CPU cores and no GPU. A single background command can run for about 2 hours, and the container is deleted when the session is idle; anything not pushed is lost. A full curriculum is estimated at 5–20 hours. The PPO network is small (two 128-wide layers), so a GPU wouldn't speed it up: stepping the physics is the bottleneck.

## Decision
- Train with Stable-Baselines3 PPO on CPU.
- Each training command runs for at most 60 minutes (`--max-minutes 60`), then saves a checkpoint and exits cleanly. `--resume` continues from the last checkpoint.
- A checkpoint holds the PPO model, the `VecNormalize` statistics, the curriculum state (stage, level, success windows), the step count and the RNG seeds.
- After each chunk, the latest and best checkpoints (well under 5 MB) are committed to an orphan branch `ckpt/<run-id>`. That branch never merges into `main`. *(Dropped by the 2026-10-17 amendment: checkpoints stay in `runs/`.)*
- Every chunk is logged in `docs/EXPERIMENTS.md`.
- The same commands work on the owner's machine as a fallback (`docs/TRAINING.md`).

## Options considered

### A: Chunked runs here, checkpoints on orphan git branches (chosen)
| Dimension | Assessment |
|---|---|
| Complexity | Low. Plain git plus a resume flag. |
| Cost | Free |
| Durability | Survives container loss; worst case loses one chunk |
| Speed | Limited by 4 cores (estimate 3k–10k steps/s) |

### B: One long run on the owner's machine
**Pros:** no chunking needed; possibly more cores. **Cons:** depends on the owner's time and machine; harder to inspect. Kept as the fallback.

### C: A rented cloud VM or GPU service
**Pros:** more cores. **Cons:** costs money and needs credentials; a GPU doesn't help this workload. Not needed for now.

### D: Store checkpoints in `main` under `runs/`
**Cons:** bloats `main`'s history forever with binaries. Rejected.

## Consequences
- Easier: a lost container costs at most one chunk; any session can pick up a run.
- Harder: the resume logic must restore everything, including the curriculum, which needs a dedicated test (stop and resume give the same state).
- Note: PPO with torch on CPU is close to repeatable but not bit-exact across resumes; the experiment log says so.
- Revisit: if measured throughput is below 3k steps/s after profiling, or the full curriculum needs more than about 30 hours.

## Action items
1. [ ] `--max-minutes`, `--resume` and a checkpoint format in `train.py` (M5).
2. [ ] Resume test in `pytest`.
3. [x] ~~`scripts/push_checkpoint.sh <run-id>` to commit to `ckpt/<run-id>`.~~ Dropped by the amendment.
4. [ ] `docs/TRAINING.md` for running on the owner's machine.

## Amendment (2026-10-17, start of M5)
The owner moved training to their own machine (8-core Apple M1 Pro, 16 GB). Nothing on that machine is reclaimed, so the orphan `ckpt/<run-id>` branches are no longer needed:
- Checkpoints (latest and best) stay in `runs/<run-id>/`, which is gitignored. No checkpoint is committed to any branch.
- `--resume` and `--max-minutes` stay: a laptop sleeps, restarts and gets closed, and the resume test still guards the curriculum state.
- The committed outputs are unchanged: `models/policy.json`, up to 8 viewer checkpoints under `web/public/checkpoints/<run-id>/` (SPEC §9.12) and the `docs/EXPERIMENTS.md` log.
- Option B above ("one long run on the owner's machine") is now the main path, still in resumable chunks.
