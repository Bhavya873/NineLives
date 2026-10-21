"""Builds the JSON that configures `ninelives_sim.BatchedSim`.

The Rust module never reads files: this reads the repo's `configs/*.toml` and
`configs/pieces.json` and passes their text, plus the mode and stage to run (see
crates/nl-py/src/config.rs).
"""

import json
import re
from pathlib import Path

# python/ninelives/sim_config.py -> the repo root is three levels up.
REPO_ROOT = Path(__file__).resolve().parents[2]
CONFIGS_DIR = REPO_ROOT / "configs"


def load_sim_config(
    mode: str = "realistic",
    stage: str = "A",
    randomize: bool = False,
    reward_overrides: dict[str, float] | None = None,
    residual: dict[str, object] | None = None,
    randomize_mode: bool = True,
) -> str:
    """The sim config JSON for `mode` ("realistic" or "arcade") and `stage`; `randomize`
    turns on SPEC §9.8 domain randomization for BatchedSim episodes (training only);
    `reward_overrides` replaces `[weights]` values of reward.toml (e.g. for an ablation);
    `residual` (`{"base": "pd", "scale": …}`) makes BatchedSim add the action to PD's
    throttles (`ResidualSettings.as_sim_json`); `randomize_mode=False` keeps `mode`'s
    preset under randomization, for a policy trained for one mode."""
    if not CONFIGS_DIR.is_dir():
        raise FileNotFoundError(
            f"configs folder not found at {CONFIGS_DIR}: install ninelives from the repo "
            "checkout with `pip install -e python` so it can find configs/*.toml"
        )
    config = {
        "vehicle_toml": (CONFIGS_DIR / "vehicle.toml").read_text(),
        "modes_toml": (CONFIGS_DIR / "modes.toml").read_text(),
        "scenarios_toml": (CONFIGS_DIR / "scenarios.toml").read_text(),
        "reward_toml": override_reward_weights(
            (CONFIGS_DIR / "reward.toml").read_text(), reward_overrides or {}
        ),
        "controllers_toml": (CONFIGS_DIR / "controllers.toml").read_text(),
        "pieces_json": (CONFIGS_DIR / "pieces.json").read_text(),
        "mode": mode,
        "stage": stage,
        "randomize": randomize,
        "randomize_mode": randomize_mode,
    }
    if residual is not None:
        config["residual"] = residual
    return json.dumps(config)


def override_reward_weights(reward_toml: str, overrides: dict[str, float]) -> str:
    """reward.toml's text with each `[weights]` entry named in `overrides` set to the
    new value. Rust still checks every weight; a name that isn't a weight is an error."""
    head, separator, rest = reward_toml.partition("[weights]")
    weights, next_header, tail = rest.partition("\n[")
    for name, value in overrides.items():
        pattern = re.compile(rf"^{re.escape(name)}\s*=.*$", re.MULTILINE)
        weights, count = pattern.subn(f"{name} = {value!r}", weights)
        if count != 1:
            raise ValueError(f"reward_overrides: `{name}` is not a [weights] entry of reward.toml")
    return head + separator + weights + next_header + tail
