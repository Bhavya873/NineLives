# ADR-001: One deterministic Rust physics core for training and the browser

**Status:** Accepted
**Date:** 2026-10-17
**Deciders:** Bhavya Modi (owner)

## Context
The same car has to behave identically in two places: in Python training (millions of episodes) and in the browser game. If those two places used different physics, a policy that lands perfectly in training could crash in the game. Tests also need repeatable results, and the ghost replay records inputs rather than positions, so it only works if the sim is deterministic.

## Decision
Write the world, vehicle, thrusters, tank, events, observations and reward once, in the Rust crate `nl-sim`, using `rapier3d` with the `enhanced-determinism` feature. Compile the same crate natively (for Python, through PyO3) and to `wasm32-unknown-unknown` (for the browser). Use only seeded `ChaCha8Rng`, never iterate over a `HashMap` in sim logic, and check a 2000-step golden trace on both targets in CI.

## Options considered

### A: One Rust core with Rapier + enhanced-determinism (chosen)
| Dimension | Assessment |
|---|---|
| Complexity | Medium. Needs a wasm build and a PyO3 build. |
| Cost | Free |
| Performance | Native is fast enough for training. Wasm has no SIMD under enhanced-determinism, so we measure it in M2. |
| Familiarity | Rust is already used in the owner's Concord project |

**Pros:** one source of truth; bit-identical native and wasm; ghost replays work.
**Cons:** two build targets; Rapier's vehicle API changes between versions.

### B: Python physics (PyBullet or MuJoCo) for training, three.js/cannon-es physics for the game
**Pros:** fast to start; well-known RL tooling.
**Cons:** two different physics engines. The policy would act on dynamics it never trained on, and the obs/reward code would be written twice. Rejected.

### C: MuJoCo compiled to wasm for both
**Pros:** excellent physics, popular in RL.
**Cons:** the wasm build is large and less mature; no raycast-vehicle helper; no bit-identical guarantee across targets. Rejected.

## Trade-off analysis
Option A costs one extra build target but removes the biggest risk in the project: a policy that works in training and fails in the game. The wasm speed risk is measurable early (M2) and has fallbacks: simpler colliders, or 90 Hz physics as a last resort.

## Consequences
- Easier: Python and TypeScript stay thin. Ghost replays and golden-trace tests are cheap.
- Harder: every physics change needs Rust, and wasm and native must stay in step.
- Revisit: if wasm ticks take more than 1.5 ms in M2.

## Action items
1. [ ] Pin `rapier3d` with `enhanced-determinism` in M0.
2. [ ] Golden trace generator and native test in M1; wasm test in M2.
