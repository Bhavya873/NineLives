"""Fingerprints that tie a result to the settings and policy it came from (plan G1).

The hashes are computed in Rust (`nl_sim::provenance`), the same code the Rust eval runner
uses, so one config gives one value in both. Kept apart from `export` so the eval command
does not need torch to start.
"""

import json
import subprocess

from ninelives.sim_config import CONFIGS_DIR, REPO_ROOT
from ninelives_sim import physics_hash, text_hash


def git_sha() -> str:
    """The commit a result was made from, with "-dirty" if configs or code differ from it
    (then the commit alone cannot reproduce the run); "unknown" outside a checkout."""
    try:
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, capture_output=True, text=True
        )
        changed = subprocess.run(
            ["git", "status", "--porcelain", "--", "configs", "crates", "python"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
    except OSError:
        return "unknown"
    sha = head.stdout.strip() or "unknown"
    return f"{sha}-dirty" if changed.stdout.strip() else sha


def provenance(sim_config_json: str, policy_json: str | None) -> dict[str, str | None]:
    """The fields every eval artifact carries: `physics_hash` (vehicle, modes, pieces and
    approach settings), `controller_config_hash` (controllers.toml), `policy_hash` (the
    exported policy text; None when the controller loads none) and `git_sha`.
    `sim_config_json` is the text `load_sim_config` built, so the hashes cover what ran."""
    config = json.loads(sim_config_json)
    approach_toml = (CONFIGS_DIR / "approach.toml").read_text()
    return {
        "physics_hash": physics_hash(
            config["vehicle_toml"], config["modes_toml"], config["pieces_json"], approach_toml
        ),
        "controller_config_hash": text_hash([config["controllers_toml"]]),
        "policy_hash": None if policy_json is None else text_hash([policy_json]),
        "git_sha": git_sha(),
    }
