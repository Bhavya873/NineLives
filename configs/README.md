# Configs

Tunable numbers live here, not in code (CONTRIBUTING.md, golden rule 6).

| File | Arrives in | Holds |
|---|---|---|
| `vehicle.toml` | M1 | chassis, wheels, suspension, drive |
| `modes.toml` | M3 | Realistic and Arcade parameters |
| `controllers.toml` | M4 | PD gains, thruster allocation weights |
| `scenarios.toml` | M4 | training stage spawn ranges (A–E), driver bot, domain randomization |
| `pieces.json` | M7 | procedural course pieces and their safe ranges (Stage D) |
| `reward.toml` | M5 | reward weights |
| `approach.toml` | G2 | when an entry onto a piece counts as clean and when a body touch is a Scrape (`docs/contracts/traversal.md`, `crash-scrape.md`) |
| `train/*.yaml` | M5 | PPO settings per run (see below) |

## Training runs (`train/`)

- `base.yaml`: the default settings every other file starts from.
- `flight-r5.yaml`: the run behind the shipped `models/policy.json` and the viewer checkpoints (the R4 policy retrained on the widened draws, 60M-step best checkpoint). `flight-r6.yaml` is the rejected retry (DECISIONS 2026-11-09). `flight-r4.yaml` is the run before, and `flight.yaml`, `flight-r3.yaml`, `flight-r3b.yaml` are its earlier rounds.
- `residual.yaml`, `residual-r1.yaml`, `residual-r2.yaml`: the residual (PD plus AI nudge) rounds from before the flight stage.
- `hover-broken.yaml`, `hover-fixed.yaml`: the two runs of the hover-hack experiment.

Each run is logged in `docs/EXPERIMENTS.md`.

Other folders: `courses/` holds course JSON (`playground.json` is the game world, `arena.json` the viewer's obstacle arena, `bumps-strip.json` a test strip, `gauntlet.json` the retired demo course kept for sim tests). `models/policy.json` is the shipped policy.
