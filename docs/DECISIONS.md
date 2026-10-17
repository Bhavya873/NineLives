# Decisions

One line per decision: date, decision, reason. Newest at the bottom. Bigger decisions get an ADR in `docs/adr/`.

| Date | Decision | Reason |
|---|---|---|
| 2026-10-17 | A hard or upside-down landing is a crash; in the game the car explodes and respawns at the last checkpoint after 1.5 s. Any roof contact at tilt > 120° crashes, at any speed. | Owner request. The 10-nozzle layout can't flip a car off its roof on flat ground anyway: every lift nozzle pushes into the ground when inverted. |
| 2026-10-17 | The stuck timer is 2 s (was 5 s). | With roof contact now a crash, a long stuck window only wastes episode time. |
| 2026-10-17 | Stage B is a side tip-over (roll 60–110°), not a roof start. | A roof start is now an instant crash, so it can't be a training scenario. |
| 2026-10-17 | `car_row` boxes are 4.5 × h × 1.9 m with 0.6 m gaps. Stage C rows hold 1–3 cars. | SPEC didn't define the size. A Realistic Hop gives about 0.5 s above 1.3 m of rise (about 13 m at 25 m/s), so 5 cars (about 25 m) need extra lift from the controller. |
| 2026-10-17 | The Gauntlet's 5-car row gets a run-up ramp (decided in detail in M7). | Keeps the demo course clearable in Realistic mode. |
| 2026-10-17 | The save *event* holds 0.5 s (§7.9); Stage A/B *success* holds 1.0 s (§9.5). Two separate timers. | The spec uses both numbers; they serve different purposes. |
| 2026-10-17 | Air mass is not added to the vehicle mass in v1. | SPEC §7.3 marks it optional in v1. Planned for M9. |
| 2026-10-17 | Train PPO on CPU in resumable 60-minute chunks; push small latest/best checkpoints to orphan `ckpt/<run-id>` branches. | The cloud container is reclaimed when idle and background commands are capped at about 2 h. See ADR-003. |
| 2026-10-17 | Deploy the game as a static site on Railway. The repo goes public once there's a working deliverable. | Owner choice. |
| 2026-10-17 | Code style targets readability by a junior developer: units in names, doc comments on every public item, comments cite SPEC sections. Replaces "comments only where the code cannot explain itself". | Owner request. See CLAUDE.md § Style. |
| 2026-10-17 | One small PR per feature from `claude/<feature>` branches. The owner is PR author and reviewer; commits are authored as the owner with a Claude co-author trailer. | Owner request. |
| 2026-10-17 | Tyre grip `friction_slip` 1.2 (was 2.5) and brake impulse 40 N·s (was 300). | Rapier limits grip to friction_slip × wheel load, so 2.5 gave about 2.5 g cornering and 300 N·s about 8 g braking. Now about 1.2 g and 1.1 g. |
| 2026-10-17 | Snapshots use postcard with a leading format version; the golden trace uses a frozen copy of vehicle.toml. | Old snapshots are refused instead of misread; tuning the car does not break the determinism check. |
| 2026-10-17 | Keep 120 Hz physics in the browser; no SIMD or collider workarounds needed. | Measured in M2 (Node 22, release wasm, flat ground): 0.017 ms per tick including pose reads, against the 1.5 ms budget. Release wasm is 0.83 MB gzipped (budget 2.5 MB). Re-measure when courses add colliders (M7). |
