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
    controller: str | None = None,
) -> str:
    """The sim config JSON for `mode` ("realistic" or "arcade") and `stage`; `randomize`
    turns on SPEC §9.8 domain randomization for BatchedSim episodes (training only);
    `reward_overrides` replaces `[weights]` values of reward.toml (e.g. for an ablation;
    `shape.<name>` names a `[shape]` value);
    `residual` (`{"base": "pd", "scale": …}`) makes BatchedSim add the action to PD's
    throttles (`ResidualSettings.as_sim_json`); `randomize_mode=False` keeps `mode`'s
    preset under randomization, for a policy trained for one mode; `controller="hybrid"`
    trains under the game's Hybrid gate (PD flies until the gate hands over)."""
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
    if controller is not None:
        config["controller"] = controller
    return json.dumps(config)


def override_reward_weights(reward_toml: str, overrides: dict[str, float]) -> str:
    """reward.toml's text with each entry named in `overrides` set to the new value.
    A plain name is a `[weights]` entry; `shape.<name>` is a `[shape]` entry. Rust still
    checks every value; a name that isn't in its table is an error."""
    for name, value in overrides.items():
        section, _, key = name.rpartition(".")
        reward_toml = set_table_entry(reward_toml, section or "weights", key, value)
    return reward_toml


def set_table_entry(toml: str, section: str, key: str, value: float) -> str:
    """`toml` with `key` in the `[section]` table set to `value`."""
    head, separator, rest = toml.partition(f"[{section}]")
    table, next_header, tail = rest.partition("\n[")
    pattern = re.compile(rf"^{re.escape(key)}\s*=.*$", re.MULTILINE)
    table, count = pattern.subn(f"{key} = {value!r}", table)
    if not separator or count != 1:
        raise ValueError(f"reward_overrides: `{key}` is not a [{section}] entry of reward.toml")
    return head + separator + table + next_header + tail
