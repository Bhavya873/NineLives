"""Paired comparison tests (SPEC §10 M7 bar): the interval and McNemar maths match known
values, runs on different seeds are refused, and each stage gets the right verdict."""

import json
from pathlib import Path

import pytest

from ninelives.compare import (
    PairedCell,
    bar_verdict,
    compare,
    mcnemar_p,
    pair_successes,
    wilson_interval,
)


def test_wilson_interval_matches_the_textbook_value():
    low, high = wilson_interval(50, 100)

    assert low == pytest.approx(0.4038, abs=1e-4)
    assert high == pytest.approx(0.5962, abs=1e-4)


def test_wilson_interval_stays_inside_zero_and_one_at_a_perfect_score():
    low, high = wilson_interval(1000, 1000)

    assert 0.99 < low < 1.0
    assert high == 1.0


def test_mcnemar_is_one_with_no_disagreements_and_small_for_a_lopsided_split():
    assert mcnemar_p(0, 0) == 1.0
    assert mcnemar_p(5, 5) == 1.0
    # Exact binomial: 2 × P(X ≤ 1 | n = 12, p = ½) = 2 × 13 / 4096.
    assert mcnemar_p(1, 11) == pytest.approx(26 / 4096)


def test_pairing_counts_the_episodes_only_one_controller_won():
    baseline = {1: True, 2: True, 3: False, 4: False}
    candidate = {1: True, 2: False, 3: True, 4: True}

    cell = pair_successes(baseline, candidate)

    assert (cell.episodes, cell.only_baseline, cell.only_candidate) == (4, 1, 2)


def test_runs_on_different_seeds_are_refused():
    with pytest.raises(ValueError, match="different seeds"):
        pair_successes({1: True}, {2: True})


def test_stage_a_needs_a_significant_win():
    big_win = PairedCell(1000, 690, 810, 28, 148)
    small_win = PairedCell(1000, 690, 700, 20, 30)

    assert bar_verdict("A", big_win) == "beats"
    assert bar_verdict("A", small_win).startswith("FAIL")


def test_a_near_perfect_baseline_only_needs_the_candidate_inside_its_interval():
    tie = PairedCell(1000, 988, 987, 1, 0)
    drop = PairedCell(1000, 988, 900, 90, 2)

    assert bar_verdict("D", tie) == "within CI"
    assert bar_verdict("D", drop).startswith("FAIL")


def write_eval(
    folder: Path,
    name: str,
    stage: str,
    mode: str,
    wins: list[bool],
    inversion_rate: float | None = None,
) -> None:
    """A minimal eval file: one row per seed with its success, and the summary's
    inversion rate when given."""
    rows = [{"seed": seed, "success": won} for seed, won in enumerate(wins)]
    report: dict = {"episodes": rows}
    if inversion_rate is not None:
        report["summary"] = {"inversion_rate": inversion_rate}
    path = folder / f"eval_{name}_{stage}_{mode}.json"
    path.write_text(json.dumps(report))


def write_both(folder: Path, stages: str, inversion_rates=(None, None)) -> None:
    """PD winning half and Hybrid every episode, on `stages` in both modes."""
    pd_inverted, hybrid_inverted = inversion_rates
    for stage in stages:
        for mode in ("realistic", "arcade"):
            write_eval(folder, "pd", stage, mode, [True, False] * 50, pd_inverted)
            write_eval(folder, "hybrid", stage, mode, [True, True] * 50, hybrid_inverted)


def test_compare_prints_a_row_per_stage_and_mode(tmp_path):
    write_both(tmp_path, "ABCDE")

    lines = compare(tmp_path, "pd", "hybrid")

    assert len(lines) == 2 + 10
    assert lines[2].startswith("| A | realistic | 50.0%")
    assert lines[2].endswith("| beats |")


def test_stage_f_rows_appear_only_when_both_controllers_have_stage_f_files(tmp_path):
    write_both(tmp_path, "ABCDE")
    write_eval(tmp_path, "pd", "F", "realistic", [True, False] * 50)

    without_f = compare(tmp_path, "pd", "hybrid")
    write_both(tmp_path, "F")
    with_f = compare(tmp_path, "pd", "hybrid")

    assert len(without_f) == 2 + 10
    assert len(with_f) == 2 + 12
    assert with_f[-1].startswith("| F | arcade |")


def test_the_inverted_column_shows_each_controllers_rate_or_a_dash(tmp_path):
    write_both(tmp_path, "ABCDE", inversion_rates=(0.45, 0.6))

    rows = compare(tmp_path, "pd", "hybrid")
    write_both(tmp_path, "ABCDE")
    old_rows = compare(tmp_path, "pd", "hybrid")

    assert "| 45% / 60% |" in rows[2]
    assert "| — / — |" in old_rows[2]
