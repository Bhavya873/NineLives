# Configs

Tunable numbers live here, not in code (CLAUDE.md, golden rule 6).

| File | Arrives in | Holds |
|---|---|---|
| `vehicle.toml` | M1 | chassis, wheels, suspension, drive |
| `modes.toml` | M3 | Realistic and Arcade parameters |
| `controllers.toml` | M4 | PD gains, thruster allocation weights |
| `scenarios.toml` | M4 | training stage spawn ranges (A–E), driver bot, domain randomization |
| `pieces.json` | M7 | procedural course pieces and their safe ranges (Stage D) |
| `reward.toml` | M5 | reward weights |
| `approach.toml` | G2 | when an entry onto a piece counts as clean and when a body touch is a Scrape (`docs/contracts/traversal.md`, `crash-scrape.md`) |
| `train/*.yaml` | M5 | PPO settings per run |
