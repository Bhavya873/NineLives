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
| `train/*.yaml` | M5 | PPO settings per run |
