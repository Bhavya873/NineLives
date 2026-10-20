"""Tunes PD's gains (configs/controllers.toml [pd]) with CMA-ES on tuning seeds (M7 round 6).

    python -m ninelives.tune_pd --generations 20 --popsize 10 --out pd-tuned.toml

Each candidate multiplies every [pd] number by exp(x) (so signs never flip) and is scored
by PD's mean success over stages A, D and E in both modes, run in Rust (`run_eval`).
Every generation draws a fresh block of tuning seeds from 2,000,000 up, disjoint from the
validation seeds (1,000,000+, 1,500,000+) and the test seeds (0–999), so the gains
cannot fit one fixed set. Writes the best candidate's controllers.toml to `--out`.
"""

import argparse
import json
import math
import re
import tomllib
from pathlib import Path

import cma

from ninelives.sim_config import CONFIGS_DIR, load_sim_config
from ninelives_sim import run_eval

# Stages where PD is not already perfect (B and C are at 100%), in both modes.
TUNING_CELLS = [(stage, mode) for stage in "ADE" for mode in ("realistic", "arcade")]
FIRST_TUNING_SEED = 2_000_000
# Up alignment is a cosine: a threshold of 1 or more would never count as upright.
MAX_UPRIGHT_ALIGNMENT = 0.99


def pd_gains(controllers_toml: str) -> dict[str, float]:
    """The [pd] numbers of a controllers.toml, in file order."""
    pd = tomllib.loads(controllers_toml)["pd"]
    return {name: float(value) for name, value in pd.items()}


def with_pd_gains(controllers_toml: str, gains: dict[str, float]) -> str:
    """`controllers_toml` with each [pd] number replaced by `gains`; other sections and
    the comments are kept."""
    head, pd_header, rest = controllers_toml.partition("[pd]")
    pd_section, next_header, tail = rest.partition("\n[")
    for name, value in gains.items():
        pd_section = re.sub(rf"(?m)^{name} = .*$", f"{name} = {value:.4g}", pd_section)
    return head + pd_header + pd_section + next_header + tail


def scaled_gains(base: dict[str, float], log_factors: list[float]) -> dict[str, float]:
    """`base` with each gain multiplied by exp(its log factor)."""
    gains = {}
    for (name, value), log_factor in zip(base.items(), log_factors, strict=True):
        gains[name] = value * math.exp(log_factor)
    gains["upright_alignment"] = min(gains["upright_alignment"], MAX_UPRIGHT_ALIGNMENT)
    return gains


def mean_success(controllers_toml: str, episodes: int, first_seed: int) -> float:
    """PD's success rate over `TUNING_CELLS`, `episodes` per cell from `first_seed`."""
    total = 0.0
    for stage, mode in TUNING_CELLS:
        config = json.loads(load_sim_config(mode, stage))
        config["controllers_toml"] = controllers_toml
        report = json.loads(run_eval("pd", json.dumps(config), episodes, first_seed))
        total += report["summary"]["success_rate"]
    return total / len(TUNING_CELLS)


def tune(generations: int, popsize: int, episodes: int, sigma: float) -> str:
    """Runs CMA-ES from the repo's gains; returns the controllers.toml of the candidate
    that beat the repo's gains by the most on its generation's seeds (the repo's own
    file if none did)."""
    base_toml = (CONFIGS_DIR / "controllers.toml").read_text()
    base = pd_gains(base_toml)
    strategy = cma.CMAEvolutionStrategy([0.0] * len(base), sigma, {"popsize": popsize})
    best_toml, best_margin = base_toml, 0.0
    for generation in range(generations):
        first_seed = FIRST_TUNING_SEED + generation * episodes
        candidates = strategy.ask()
        tomls = [with_pd_gains(base_toml, scaled_gains(base, list(x))) for x in candidates]
        scores = [mean_success(text, episodes, first_seed) for text in tomls]
        strategy.tell(candidates, [-score for score in scores])
        repo_score = mean_success(base_toml, episodes, first_seed)
        top = max(range(len(scores)), key=lambda index: scores[index])
        if scores[top] - repo_score > best_margin:
            best_toml, best_margin = tomls[top], scores[top] - repo_score
        print(f"generation {generation}: best {scores[top]:.3f}, repo {repo_score:.3f}", flush=True)
    return best_toml


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--generations", type=int, default=20)
    parser.add_argument("--popsize", type=int, default=10)
    parser.add_argument("--episodes", type=int, default=200, help="per stage and mode")
    parser.add_argument("--sigma", type=float, default=0.3, help="initial log-factor step")
    parser.add_argument("--out", type=Path, required=True, help="controllers.toml to write")
    args = parser.parse_args(argv)
    best = tune(args.generations, args.popsize, args.episodes, args.sigma)
    args.out.write_text(best)
    print(f"wrote {args.out}:\n" + json.dumps(pd_gains(best), indent=2))


if __name__ == "__main__":
    main()
