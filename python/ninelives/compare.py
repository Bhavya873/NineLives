"""Paired comparison of two controllers' eval files, judged against the M7 bar (SPEC §10).

    python -m ninelives.compare --baseline pd --candidate hybrid [--results results]

Both controllers must have been run on the same seeds (`ninelives.eval` or the Rust
runner). Per stage and mode it prints success with a Wilson 95% interval, the episodes
only one of them won, an exact McNemar p-value on those, and the bar's verdict: beat the
baseline on A and E, stay within its interval where it succeeds at least 97% of the time.
"""

import argparse
import json
import math
from dataclasses import dataclass
from pathlib import Path

from ninelives.sim_config import REPO_ROOT

STAGES = ("A", "B", "C", "D", "E")
MODES = ("realistic", "arcade")
# z for a two-sided 95% interval.
Z_95 = 1.96
# The candidate must beat the baseline on these stages (SPEC §10 M7 bar).
MUST_BEAT_STAGES = ("A", "E")
# Where the baseline succeeds this often, staying within its interval is enough.
NEAR_PERFECT_SUCCESS = 0.97
# Significance level for "beats".
SIGNIFICANCE = 0.05


@dataclass(frozen=True)
class PairedCell:
    """One stage and mode: both controllers on the same seeds."""

    episodes: int
    baseline_wins: int
    candidate_wins: int
    only_baseline: int
    only_candidate: int

    @property
    def baseline_rate(self) -> float:
        return self.baseline_wins / self.episodes

    @property
    def candidate_rate(self) -> float:
        return self.candidate_wins / self.episodes


def wilson_interval(successes: int, episodes: int) -> tuple[float, float]:
    """Wilson score 95% interval for a success rate (stays inside 0..1 near 0% and 100%)."""
    p = successes / episodes
    z2 = Z_95 * Z_95
    centre = (p + z2 / (2 * episodes)) / (1 + z2 / episodes)
    spread = math.sqrt(p * (1 - p) / episodes + z2 / (4 * episodes * episodes))
    half = Z_95 * spread / (1 + z2 / episodes)
    return max(0.0, centre - half), min(1.0, centre + half)


def mcnemar_p(only_baseline: int, only_candidate: int) -> float:
    """Exact two-sided McNemar p-value: a fair-coin binomial test on the discordant pairs."""
    discordant = only_baseline + only_candidate
    if discordant == 0:
        return 1.0
    fewer = min(only_baseline, only_candidate)
    tail = sum(math.comb(discordant, k) for k in range(fewer + 1)) / 2**discordant
    return min(1.0, 2 * tail)


def pair_successes(baseline: dict[int, bool], candidate: dict[int, bool]) -> PairedCell:
    """Pairs two controllers' per-seed successes; refuses runs on different seeds."""
    if set(baseline) != set(candidate):
        raise ValueError(
            "the two eval files cover different seeds; rerun both with the same "
            "--seed and --episodes"
        )
    seeds = sorted(baseline)
    return PairedCell(
        episodes=len(seeds),
        baseline_wins=sum(baseline[s] for s in seeds),
        candidate_wins=sum(candidate[s] for s in seeds),
        only_baseline=sum(baseline[s] and not candidate[s] for s in seeds),
        only_candidate=sum(candidate[s] and not baseline[s] for s in seeds),
    )


def bar_verdict(stage: str, cell: PairedCell) -> str:
    """The M7 bar (SPEC §10) for one cell: 'beats', 'within CI' or why it fails."""
    p = mcnemar_p(cell.only_baseline, cell.only_candidate)
    beats = cell.candidate_rate > cell.baseline_rate and p < SIGNIFICANCE
    if stage in MUST_BEAT_STAGES or cell.baseline_rate < NEAR_PERFECT_SUCCESS:
        return "beats" if beats else "FAIL: must beat the baseline"
    low, _ = wilson_interval(cell.baseline_wins, cell.episodes)
    if cell.candidate_rate >= low:
        return "within CI"
    return "FAIL: below the baseline's interval"


def successes_by_seed(path: Path) -> dict[int, bool]:
    """Each episode's success in an eval file, keyed by seed."""
    report = json.loads(path.read_text())
    return {row["seed"]: bool(row["success"]) for row in report["episodes"]}


def table_row(stage: str, mode: str, cell: PairedCell) -> str:
    """One markdown table row for a cell."""
    base_low, base_high = wilson_interval(cell.baseline_wins, cell.episodes)
    cand_low, cand_high = wilson_interval(cell.candidate_wins, cell.episodes)
    p = mcnemar_p(cell.only_baseline, cell.only_candidate)
    return (
        f"| {stage} | {mode} | {cell.baseline_rate:.1%} [{base_low:.1%}–{base_high:.1%}] "
        f"| {cell.candidate_rate:.1%} [{cand_low:.1%}–{cand_high:.1%}] "
        f"| {cell.only_baseline} / {cell.only_candidate} | {p:.2g} "
        f"| {bar_verdict(stage, cell)} |"
    )


def compare(results_dir: Path, baseline: str, candidate: str, suffix: str = "") -> list[str]:
    """The markdown table for every stage and mode, from `eval_<name>_<stage>_<mode>`
    files in `results_dir` (`suffix` is `_dr` for randomized runs)."""
    lines = [
        f"| Stage | Mode | {baseline} | {candidate} | only {baseline} / only {candidate} "
        "| McNemar p | M7 bar |",
        "|---|---|---|---|---|---|---|",
    ]
    for stage in STAGES:
        for mode in MODES:
            stem = f"_{stage}_{mode}{suffix}.json"
            cell = pair_successes(
                successes_by_seed(results_dir / f"eval_{baseline}{stem}"),
                successes_by_seed(results_dir / f"eval_{candidate}{stem}"),
            )
            lines.append(table_row(stage, mode, cell))
    return lines


def main(argv: list[str] | None = None) -> None:
    """The command line (see the module docstring)."""
    parser = argparse.ArgumentParser(description="Paired comparison against the M7 bar.")
    parser.add_argument("--baseline", default="pd")
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--results", type=Path, default=REPO_ROOT / "results")
    parser.add_argument("--dr", action="store_true", help="compare the _dr files")
    args = parser.parse_args(argv)
    suffix = "_dr" if args.dr else ""
    print("\n".join(compare(args.results, args.baseline, args.candidate, suffix)))


if __name__ == "__main__":
    main()
