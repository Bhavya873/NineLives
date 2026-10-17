# ADR-002: Python talks to the sim through one batched PyO3 call

**Status:** Accepted
**Date:** 2026-10-17
**Deciders:** Bhavya Modi (owner)

## Context
PPO collects about 32,000 steps per update across 64 environments. If Python called into Rust once per environment per step, the call overhead and Python loops would cost more than the physics itself. Training runs on a 4-core CPU, so the sim must use every core.

## Decision
The PyO3 module `ninelives_sim` exposes one class, `BatchedSim(n_envs, config_json, seed)`. `step(actions)` takes an `(N, 10)` float32 numpy array and returns `(N, 41)` observations plus reward, done and info arrays, all as numpy arrays. Inside Rust, `rayon` steps the N independent worlds in parallel. Each world owns its own Rapier pipeline and RNG; Rapier's own `parallel` feature stays off. Finished environments auto-reset inside Rust and return their final observation in `info["terminal_obs"]`. A thin `NineLivesVecEnv` adapts this to Stable-Baselines3.

## Options considered

### A: Batched PyO3 + rayon (chosen)
| Dimension | Assessment |
|---|---|
| Complexity | Medium (PyO3 + maturin build) |
| Performance | One FFI call per step for all envs; all cores busy |
| Correctness risk | Low. Tested against N single envs. |

### B: One Gymnasium env per process (SB3 `SubprocVecEnv`)
**Pros:** standard and simple. **Cons:** pickling and inter-process messages on every step; much slower for a sim this fast. Rejected.

### C: A socket or gRPC server around the sim
**Pros:** language-neutral. **Cons:** network overhead and an extra service to run. Rejected.

## Consequences
- Easier: fast training on a laptop-class CPU.
- Harder: auto-reset semantics must exactly match SB3's expectations; covered by a dedicated test.
- Revisit: if profiling in M5 shows the numpy copies dominate.

## Action items
1. [ ] `BatchedSim` and `NineLivesVecEnv` in M5.
2. [ ] Tests: batched equals 4 single envs; auto-reset terminal obs.
