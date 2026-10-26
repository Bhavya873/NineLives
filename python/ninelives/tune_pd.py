"""Tunes PD's gains (configs/controllers.toml [pd]) with CMA-ES on tuning seeds (M7 round 6).

    python -m ninelives.tune_pd --generations 20 --popsize 10 --out pd-tuned.toml

Each candidate multiplies every tunable [pd] number (how hard PD acts, not when; see
`UNTUNED`) by exp(x) (so signs never flip) and is scored by PD's mean success over stages
A, D and E in both modes, each on the plain car and with domain randomization (SPEC
§9.8), run in Rust (`run_eval`), less a cost for air used over the bumps strip past its
limit (owner, round 7: no firing whenever a wheel lifts). Scored on the plain car alone,
the search found huge gains that burned most of the tank and fell apart on randomized
cars (D 93% → 40%), so the randomized episodes keep it honest.
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

from ninelives.sim_config import CONFIGS_DIR, REPO_ROOT, load_sim_config
from ninelives_sim import driving_air_kg, run_eval

# Stages where PD is not already perfect (B and C are at 100%), in both modes, on the
# plain car and with domain randomization.
TUNING_CELLS = [
    (stage, mode, randomize)
    for stage in "ADE"
    for mode in ("realistic", "arcade")
    for randomize in (False, True)
]
FIRST_TUNING_SEED = 2_000_000
# Gains controllers.toml requires to be fractions (an up alignment is a cosine; the others
# are shares of lift or weight), capped just below 1.
FRACTION_GAINS = ("ground_side_push",)
MAX_FRACTION = 0.99


# Never tuned, so the search changes how hard PD acts but not when (round 7):
# - the air-control feel (eval presses no keys, so these would drift at random);
# - the angular-acceleration cap that keeps the passenger g-load down;
# - the rules for when PD acts at all. Tuned freely, v3 braked any fall faster than
#   0.4 m/s and fired on bumps (17 kg over the bumps test; owner: it must not fire
#   whenever a wheel lifts), and braked too late to stop a fast fall. v4 kept those but
#   raised brake_usable_fraction to 0.99, starting fast-fall braking lower.
UNTUNED = (
    "air_yaw_rate_rad_s",
    "air_pitch_rad",
    "air_wish_min_height_m",
    "max_angular_accel_rad_s2",
    "upright_alignment",
    "brake_speed_m_s",
    "brake_safety",
    "brake_height_m",
    "brake_margin_m",
    "brake_usable_fraction",
    "lying_max_fall_speed_m_s",
    "quiet_max_tilt_deg",
    "quiet_max_spin_rad_s",
)


def pd_gains(controllers_toml: str) -> dict[str, float]:
    """The tunable [pd] numbers of a controllers.toml, in file order."""
    pd = tomllib.loads(controllers_toml)["pd"]
    return {name: float(value) for name, value in pd.items() if name not in UNTUNED}


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
    for name in FRACTION_GAINS:
        if name in gains:
            gains[name] = min(gains[name], MAX_FRACTION)
    return gains


# Air used driving 8 s over the bumps strip (the PD bumps test's course and limit):
# a candidate pays for every kg past the limit, in either mode. v4 used 10.6 kg there
# (the repo's PD 6.7) to gain 0.046 success; at 0.05 per kg that gain is cancelled.
BUMPS_COURSE = REPO_ROOT / "courses" / "bumps-strip.json"
BUMPS_SECONDS = 8.0
BUMPS_AIR_LIMIT_KG = 8.0
BUMPS_COST_PER_KG = 0.05


def bumps_air_kg(controllers_toml: str) -> float:
    """The most air PD uses over the bumps strip, Realistic or Arcade, in kg."""
    course_json = BUMPS_COURSE.read_text()
    worst_kg = 0.0
    for mode in ("realistic", "arcade"):
        config = json.loads(load_sim_config(mode))
        config["controllers_toml"] = controllers_toml
        used_kg = driving_air_kg("pd", json.dumps(config), course_json, BUMPS_SECONDS)
        worst_kg = max(worst_kg, used_kg)
    return worst_kg


def score(controllers_toml: str, episodes: int, first_seed: int) -> float:
    """Mean success over `TUNING_CELLS`, less the cost of air past the bumps limit."""
    over_kg = max(0.0, bumps_air_kg(controllers_toml) - BUMPS_AIR_LIMIT_KG)
    return mean_success(controllers_toml, episodes, first_seed) - BUMPS_COST_PER_KG * over_kg


def mean_success(controllers_toml: str, episodes: int, first_seed: int) -> float:
    """PD's success rate over `TUNING_CELLS`, `episodes` per cell from `first_seed`."""
    total = 0.0
    for stage, mode, randomize in TUNING_CELLS:
        config = json.loads(load_sim_config(mode, stage, randomize))
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
        scores = [score(text, episodes, first_seed) for text in tomls]
        strategy.tell(candidates, [-score for score in scores])
        repo_score = score(base_toml, episodes, first_seed)
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
