"""Builds the JSON that configures `ninelives_sim.BatchedSim`.

The Rust module never reads files: this reads the repo's `configs/*.toml` and
`configs/pieces.json` and passes their text, plus the mode and stage to run (see
crates/nl-py/src/config.rs).
"""

import json
from pathlib import Path

# python/ninelives/sim_config.py -> the repo root is three levels up.
REPO_ROOT = Path(__file__).resolve().parents[2]
CONFIGS_DIR = REPO_ROOT / "configs"


def load_sim_config(mode: str = "realistic", stage: str = "A", randomize: bool = False) -> str:
    """The sim config JSON for `mode` ("realistic" or "arcade") and `stage`; `randomize`
    turns on SPEC §9.8 domain randomization for BatchedSim episodes (training only)."""
    if not CONFIGS_DIR.is_dir():
        raise FileNotFoundError(
            f"configs folder not found at {CONFIGS_DIR}: install ninelives from the repo "
            "checkout with `pip install -e python` so it can find configs/*.toml"
        )
    config = {
        "vehicle_toml": (CONFIGS_DIR / "vehicle.toml").read_text(),
        "modes_toml": (CONFIGS_DIR / "modes.toml").read_text(),
        "scenarios_toml": (CONFIGS_DIR / "scenarios.toml").read_text(),
        "reward_toml": (CONFIGS_DIR / "reward.toml").read_text(),
        "controllers_toml": (CONFIGS_DIR / "controllers.toml").read_text(),
        "pieces_json": (CONFIGS_DIR / "pieces.json").read_text(),
        "mode": mode,
        "stage": stage,
        "randomize": randomize,
    }
    return json.dumps(config)
