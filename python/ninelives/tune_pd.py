"""Tunes PD's gains (configs/controllers.toml [pd], plus the allocation's
lag_compensation) with CMA-ES on tuning seeds (M7 round 6; control upgrade 2026-10-26).

    python -m ninelives.tune_pd --generations 30 --popsize 12 --keep 5 --out pd-tuned.toml

Each candidate multiplies every tunable number (how hard PD acts, not when; see
`UNTUNED`) by exp(x) (so signs never flip) and is scored by PD's mean success over stages
A–F in both modes, each on the plain car and with domain randomization (SPEC §9.8), run
in Rust (`run_eval`), less a cost for air used over the bumps strip past its limit
(owner, round 7: no firing whenever a wheel lifts). Scored on the plain car alone,
the search found huge gains that burned most of the tank and fell apart on randomized
cars (D 93% → 40%), so the randomized episodes keep it honest.
Every generation draws a fresh block of tuning seeds from 2,000,000 up, disjoint from the
validation seeds (1,000,000+, 1,500,000+) and the test seeds (0–999), so the gains
cannot fit one fixed set. Writes the best candidate's controllers.toml to `--out`, and
with `--keep N` the next best and CMA-ES's final mean beside it, for validation.
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

# Every stage, in both modes, on the plain car and with domain randomization: B and C
# are no longer at 100% on the new car, and F (strong flip plates) is where PD gains most.
TUNING_CELLS = [
    (stage, mode, randomize)
    for stage in "ABCDEF"
    for mode in ("realistic", "arcade")
    for randomize in (False, True)
]
FIRST_TUNING_SEED = 2_000_000
# The one-time final test block (3,000,000–3,000,999, DECISIONS 2026-10-21): tuning must
# never touch it.
FINAL_TEST_FIRST_SEED = 3_000_000
# Gains controllers.toml requires to be fractions (an up alignment is a cosine; the others
# are shares of lift or weight), capped just below 1.
FRACTION_GAINS = ("ground_side_push", "lag_compensation")
MAX_FRACTION = 0.99
# The [allocation] numbers tuned beside [pd] (none of its names is a [pd] name).
TUNED_ALLOCATION = ("lag_compensation",)
# PD blends from its upright to its tumble gains between these tilts; keep them apart.
BLEND_START, BLEND_END = "tumble_blend_start_deg", "tumble_blend_end_deg"
MIN_BLEND_WIDTH_DEG = 5.0
MAX_BLEND_END_DEG = 170.0


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
    "on_ground_height_m",
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
    return with_section_values(controllers_toml, "pd", gains)


def with_section_values(controllers_toml: str, section: str, values: dict[str, float]) -> str:
    """`controllers_toml` with each number of `[section]` named in `values` replaced."""
    head, header, rest = controllers_toml.partition(f"[{section}]")
    table, next_header, tail = rest.partition("\n[")
    for name, value in values.items():
        table = re.sub(rf"(?m)^{name} = .*$", f"{name} = {value:.4g}", table)
    return head + header + table + next_header + tail


def tunables(controllers_toml: str) -> dict[str, float]:
    """Every tuned number: the [pd] gains, then `TUNED_ALLOCATION`."""
    allocation = tomllib.loads(controllers_toml)["allocation"]
    values = pd_gains(controllers_toml)
    for name in TUNED_ALLOCATION:
        values[name] = float(allocation[name])
    return values


def with_tunables(controllers_toml: str, values: dict[str, float]) -> str:
    """`controllers_toml` with the [pd] and [allocation] numbers in `values` replaced."""
    allocation = {name: values[name] for name in TUNED_ALLOCATION if name in values}
    gains = {name: value for name, value in values.items() if name not in allocation}
    return with_section_values(with_pd_gains(controllers_toml, gains), "allocation", allocation)


def scaled_gains(base: dict[str, float], log_factors: list[float]) -> dict[str, float]:
    """`base` with each gain multiplied by exp(its log factor)."""
    gains = {}
    for (name, value), log_factor in zip(base.items(), log_factors, strict=True):
        gains[name] = value * math.exp(log_factor)
    for name in FRACTION_GAINS:
        if name in gains:
            gains[name] = min(gains[name], MAX_FRACTION)
    if BLEND_START in gains and BLEND_END in gains:
        # Scaled apart, the band could close or flip; controllers.toml needs end > start.
        lowest_end_deg = gains[BLEND_START] + MIN_BLEND_WIDTH_DEG
        gains[BLEND_END] = min(max(gains[BLEND_END], lowest_end_deg), MAX_BLEND_END_DEG)
        gains[BLEND_START] = min(gains[BLEND_START], gains[BLEND_END] - MIN_BLEND_WIDTH_DEG)
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


def tuning_seeds_start(generation: int, episodes: int) -> int:
    """The first tuning seed of `generation`; refuses a block that would reach the final
    test seeds."""
    first_seed = FIRST_TUNING_SEED + generation * episodes
    if first_seed + episodes > FINAL_TEST_FIRST_SEED:
        raise ValueError(
            f"tune_pd: generation {generation} with {episodes} episodes would use seeds "
            f"up to {first_seed + episodes - 1}, inside the final test block "
            f"({FINAL_TEST_FIRST_SEED:,}+); use fewer generations or episodes"
        )
    return first_seed


def tune(generations: int, popsize: int, episodes: int, sigma: float) -> list[str]:
    """Runs CMA-ES from the repo's gains. Returns controllers.tomls, best first: the
    candidates that beat the repo's gains on their generation's seeds, by margin (the
    repo's own file if none did), then CMA-ES's final mean."""
    base_toml = (CONFIGS_DIR / "controllers.toml").read_text()
    base = tunables(base_toml)
    strategy = cma.CMAEvolutionStrategy([0.0] * len(base), sigma, {"popsize": popsize})
    winners: list[tuple[float, str]] = []
    # Refuse a run whose last generation would reach the test block before it starts.
    tuning_seeds_start(generations - 1, episodes)
    for generation in range(generations):
        first_seed = tuning_seeds_start(generation, episodes)
        candidates = strategy.ask()
        tomls = [with_tunables(base_toml, scaled_gains(base, list(x))) for x in candidates]
        scores = [score(text, episodes, first_seed) for text in tomls]
        strategy.tell(candidates, [-score for score in scores])
        repo_score = score(base_toml, episodes, first_seed)
        for text, candidate_score in zip(tomls, scores, strict=True):
            if candidate_score > repo_score:
                winners.append((candidate_score - repo_score, text))
        top = max(scores)
        print(f"generation {generation}: best {top:.3f}, repo {repo_score:.3f}", flush=True)
    winners.sort(key=lambda winner: winner[0], reverse=True)
    best_first = [text for _, text in winners] or [base_toml]
    mean = list(strategy.result.xfavorite)
    return best_first + [with_tunables(base_toml, scaled_gains(base, mean))]


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--generations", type=int, default=20)
    parser.add_argument("--popsize", type=int, default=10)
    parser.add_argument("--episodes", type=int, default=200, help="per stage and mode")
    parser.add_argument("--sigma", type=float, default=0.3, help="initial log-factor step")
    parser.add_argument("--out", type=Path, required=True, help="controllers.toml to write")
    parser.add_argument(
        "--keep", type=int, default=0, help="also write the next N best and the CMA-ES mean"
    )
    args = parser.parse_args(argv)
    ranked = tune(args.generations, args.popsize, args.episodes, args.sigma)
    args.out.write_text(ranked[0])
    print(f"wrote {args.out}:\n" + json.dumps(tunables(ranked[0]), indent=2))
    if args.keep > 0:
        for rank, text in enumerate(ranked[1:-1][: args.keep], start=2):
            args.out.with_name(f"{args.out.stem}-top{rank}.toml").write_text(text)
        args.out.with_name(f"{args.out.stem}-mean.toml").write_text(ranked[-1])


if __name__ == "__main__":
    main()
