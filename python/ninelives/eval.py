"""Fixed-seed evaluation of Off, PD or the trained AI (SPEC §10).

    python -m ninelives.eval --controller ai --stage A --mode realistic --episodes 1000 --seed 0
    ... --dr   # with domain randomization (SPEC §9.8); off by default

Episodes run in Rust (`ninelives_sim.run_eval`, the same code as the Rust eval runner),
so the numbers match it exactly. Writes `results/eval_<controller>_<stage>_<mode>.json`
(summary plus one row per episode) and a `.csv` of the rows.
"""

import argparse
import csv
import json
from pathlib import Path
from typing import Any

from ninelives.run_files import DEFAULT_POLICY_PATH
from ninelives.sim_config import REPO_ROOT, load_sim_config
from ninelives_sim import run_eval

RESULTS_DIR = REPO_ROOT / "results"


def evaluate(
    controller: str,
    stage: str,
    mode: str,
    episodes: int,
    first_seed: int,
    policy: Path = DEFAULT_POLICY_PATH,
    randomize: bool = False,
) -> dict[str, Any]:
    """The eval report: settings, the SPEC §10 summary and every episode's row.
    `policy` is the exported policy file the AI controller loads; `randomize` gives
    every episode its own randomized car and world (SPEC §9.8)."""
    policy_json = policy.read_text() if controller == "ai" else None
    config = load_sim_config(mode, stage, randomize)
    report = json.loads(run_eval(controller, config, episodes, first_seed, policy_json))
    return {
        "controller": controller,
        "stage": stage,
        "mode": mode,
        "first_seed": first_seed,
        "randomized": randomize,
        "summary": report["summary"],
        "episodes": report["episodes"],
    }


def write_report(report: dict[str, Any], results_dir: Path = RESULTS_DIR) -> Path:
    """Writes the report's .json and .csv; returns the .json path."""
    results_dir.mkdir(parents=True, exist_ok=True)
    stem = f"eval_{report['controller']}_{report['stage']}_{report['mode']}"
    if report["randomized"]:
        stem += "_dr"
    json_path = results_dir / f"{stem}.json"
    # Compact: 1000 rows pretty-printed would be several hundred KB.
    json_path.write_text(json.dumps(report, separators=(",", ":")))
    rows = report["episodes"]
    with (results_dir / f"{stem}.csv").open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    return json_path


def positive_int(text: str) -> int:
    """An argparse type: a whole number of at least 1."""
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError(f"must be at least 1, not {value}")
    return value


def main(argv: list[str] | None = None) -> None:
    """The command line (see the module docstring)."""
    parser = argparse.ArgumentParser(description="Evaluate a controller on fixed seeds.")
    parser.add_argument("--controller", choices=["off", "pd", "ai"], required=True)
    parser.add_argument("--stage", choices=["A", "B"], default="A")
    parser.add_argument("--mode", choices=["realistic", "arcade"], default="realistic")
    parser.add_argument("--episodes", type=positive_int, default=1000)
    parser.add_argument("--seed", type=int, default=0, help="first episode seed")
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY_PATH, help="for ai")
    parser.add_argument("--dr", action="store_true", help="domain randomization (SPEC §9.8)")
    args = parser.parse_args(argv)
    report = evaluate(
        args.controller, args.stage, args.mode, args.episodes, args.seed, args.policy, args.dr
    )
    print(json.dumps(report["summary"], indent=2))
    print(f"wrote {write_report(report)}")


if __name__ == "__main__":
    main()
