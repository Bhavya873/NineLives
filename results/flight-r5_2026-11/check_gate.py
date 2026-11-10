"""Checks the revised r5 promotion rule (docs/DECISIONS.md, 2026-11-09) against the compact files
in this folder, with no other input:

    python results/flight-r5_2026-11/check_gate.py              # every block and candidate
    python results/flight-r5_2026-11/check_gate.py seeds_4000000 r6

A cell is Hybrid success_rate (percent). The four conditions, new policy against the old one:
1. the 12-cell mean on the widened draws is higher;
2. no cell of the legacy (pre-widening) eval is worse by more than 1 point;
3. no widened cell is worse by more than 1.5 points;
4. Stage F limited air (realistic) is better by more than 2 standard errors of the difference.
"""

import json
import math
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CELLS = [(stage, mode) for stage in "ABCDEF" for mode in ("realistic", "arcade")]
CANDIDATES = {
    "seeds_1500000": ["r5"],
    "seeds_3000000": ["r5"],
    "seeds_4000000": ["r5", "r6"],
}


def rates(path: Path, kind: str) -> dict[tuple[str, str], float]:
    cells = json.loads(path.read_text())[kind]
    return {
        (stage, mode): cells[f"hybrid_{stage}_{mode}"]["summary"]["success_rate"] * 100
        for stage, mode in CELLS
    }


def check(block: str, candidate: str) -> bool:
    old_w, new_w = (rates(HERE / block / f"{name}.json", "widened") for name in ("old", candidate))
    old_l, new_l = (rates(HERE / block / f"{name}.json", "legacy") for name in ("old", candidate))
    mean_old, mean_new = sum(old_w.values()) / 12, sum(new_w.values()) / 12
    drops_l = {cell: old_l[cell] - new_l[cell] for cell in CELLS}
    drops_w = {cell: old_w[cell] - new_w[cell] for cell in CELLS}
    worst_l = max(drops_l, key=drops_l.get)
    worst_w = max(drops_w, key=drops_w.get)
    p_old, p_new = old_w[("F", "realistic")] / 100, new_w[("F", "realistic")] / 100
    se = math.sqrt(p_old * (1 - p_old) / 1000 + p_new * (1 - p_new) / 1000) * 100
    gain = (p_new - p_old) * 100
    results = [
        mean_new > mean_old,
        drops_l[worst_l] <= 1,
        drops_w[worst_w] <= 1.5,
        gain > 2 * se,
    ]
    word = {True: "PASS", False: "FAIL"}
    print(f"== {block}: old vs {candidate}")
    print(f"(1) widened 12-cell mean {mean_old:.2f} -> {mean_new:.2f}: {word[results[0]]}")
    print(f"(2) legacy worst cell {worst_l} {-drops_l[worst_l]:+.1f}: {word[results[1]]}")
    print(f"(3) widened worst cell {worst_w} {-drops_w[worst_w]:+.1f}: {word[results[2]]}")
    print(f"(4) Stage F limited air {p_old * 100:.1f} -> {p_new * 100:.1f} ({gain:+.1f}, standard error {se:.2f}): {word[results[3]]}")
    print(f"    all four: {word[all(results)]}\n")
    return all(results)


if __name__ == "__main__":
    pairs = [(sys.argv[1], sys.argv[2])] if len(sys.argv) == 3 else [
        (block, name) for block, names in CANDIDATES.items() for name in names
    ]
    for block, name in pairs:
        check(block, name)
