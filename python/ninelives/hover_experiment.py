"""Reports the hover-hack experiment (SPEC §9.11): the broken reward against the fixed one.

Both runs come from `configs/train/hover-broken.yaml` and `hover-fixed.yaml` (Stage A,
Arcade, the same seed and budget). This flies each run's final policy deterministically on
the same fixed seeds at Stage A's hardest level and measures how it ends every episode and
how long it stays in the air, then writes a JSON report and an SVG of the heights.

    python -m ninelives.hover_experiment --broken runs/hover-broken --fixed runs/hover-fixed \
        --out results/hover_hack.json --svg docs/images/hover-hack.svg
"""

import argparse
import json
import logging
from pathlib import Path
from typing import Any

import numpy as np
from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import VecNormalize

from ninelives.env import finished_episode
from ninelives.run_files import load_model_and_normalizer
from ninelives.sim_config import load_sim_config
from ninelives_sim import BatchedSim, obs_spec

log = logging.getLogger(__name__)

STAGE = "A"
MODE = "arcade"
EPISODES = 200
FIRST_SEED = 2_000_000
TRACES = 3

_FIELDS = [field["name"] for field in obs_spec()]
HEIGHT = _FIELDS.index("height")
WHEELS = [_FIELDS.index(f"wheel_contact.{wheel}") for wheel in ("fl", "fr", "rl", "rr")]


def rollout(
    model: PPO, normalizer: VecNormalize, episodes: int = EPISODES, first_seed: int = FIRST_SEED
) -> dict[str, Any]:
    """Flies one deterministic episode per seed; per-episode metrics and a few height traces."""
    config = load_sim_config(MODE, STAGE)
    sim = BatchedSim(episodes, config, first_seed)
    level = sim.level_count(STAGE) - 1
    sim.set_stage_and_level(STAGE, level)
    seeds = np.arange(first_seed, first_seed + episodes, dtype=np.uint64)
    observations = sim.reset(seeds)
    rows: list[dict[str, Any] | None] = [None] * episodes
    steps = np.zeros(episodes, dtype=np.int64)
    airborne = np.zeros(episodes, dtype=np.int64)
    traces: list[list[float]] = [[] for _ in range(TRACES)]
    while None in rows:
        actions, _ = model.predict(normalizer.normalize_obs(observations), deterministic=True)
        observations, _, _, _, info = sim.step(actions.astype(np.float32))
        for row in range(episodes):
            if rows[row] is not None:
                continue
            seen = info["terminal_obs"][row] if info["done"][row] else observations[row]
            steps[row] += 1
            airborne[row] += int(all(seen[wheel] < 0.5 for wheel in WHEELS))
            if row < TRACES:
                traces[row].append(float(seen[HEIGHT]))
            if info["done"][row]:
                rows[row] = finished_episode(info, row)
    finished = [row for row in rows if row is not None]
    seconds_per_step = np.array([row["duration_s"] for row in finished]) / steps
    flight_s = airborne * seconds_per_step
    duration_s = np.array([row["duration_s"] for row in finished])
    success = np.array([bool(row["success"]) for row in finished])
    crash = np.array([row["crash_cause"] is not None for row in finished])
    return {
        "episodes": episodes,
        "level": level,
        "success_rate": float(success.mean()),
        "crash_rate": float(crash.mean()),
        "timeout_rate": float((~success & ~crash).mean()),
        "mean_flight_s": float(flight_s.mean()),
        "median_flight_s": float(np.median(flight_s)),
        "max_flight_s": float(flight_s.max()),
        "mean_duration_s": float(duration_s.mean()),
        "mean_air_used_kg": float(np.mean([row["air_used_kg"] for row in finished])),
        "step_s": float(seconds_per_step.mean()),
        "height_traces": [[round(value, 3) for value in trace] for trace in traces],
    }


def report(broken_run: Path, fixed_run: Path) -> dict[str, Any]:
    """Both runs' metrics plus how each run was trained."""
    result: dict[str, Any] = {
        "stage": STAGE,
        "mode": MODE,
        "first_seed": FIRST_SEED,
        "episodes": EPISODES,
    }
    for label, run in (("broken", broken_run), ("fixed", fixed_run)):
        model, normalizer = load_model_and_normalizer(run / "latest")
        state = json.loads((run / "latest" / "state.json").read_text())
        metrics = rollout(model, normalizer)
        metrics["train_steps"] = state["steps"]
        metrics["train_wall_s"] = round(state["wall_seconds"])
        result[label] = metrics
    return result


def svg_of(result: dict[str, Any]) -> str:
    """Height against time for each run's first traces, the headline numbers underneath."""
    width, panel_h, margin = 760, 200, 46
    longest = max(
        len(trace) * result[label]["step_s"]
        for label in ("broken", "fixed")
        for trace in result[label]["height_traces"]
    )
    top = max(
        (
            max(trace, default=1.0)
            for label in ("broken", "fixed")
            for trace in result[label]["height_traces"]
        ),
        default=1.0,
    )
    colors = ["#e4572e", "#2e86ab", "#76b041"]
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {2 * panel_h + 90}" '
        'font-family="system-ui, sans-serif" font-size="12">',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
    ]
    titles = {
        "broken": "Broken reward (+0.1 per upright step, no airtime cost)",
        "fixed": "Fixed reward (SPEC 9.4)",
    }
    for index, label in enumerate(("broken", "fixed")):
        y0 = 28 + index * (panel_h + 20)
        parts.append(f'<text x="{margin}" y="{y0 - 10}" font-weight="600">{titles[label]}</text>')
        parts.append(
            f'<rect x="{margin}" y="{y0}" width="{width - margin - 16}" height="{panel_h - 40}" '
            'fill="none" stroke="#999"/>'
        )
        step_s = result[label]["step_s"]
        for trace_index, trace in enumerate(result[label]["height_traces"]):
            points = " ".join(
                f"{margin + (i * step_s / longest) * (width - margin - 16):.1f},"
                f"{y0 + (panel_h - 40) * (1 - value / top):.1f}"
                for i, value in enumerate(trace)
            )
            parts.append(
                f'<polyline points="{points}" fill="none" stroke="{colors[trace_index]}" stroke-width="1.6"/>'
            )
        parts.append(f'<text x="{margin}" y="{y0 + panel_h - 22}" fill="#555">0 s</text>')
        parts.append(
            f'<text x="{width - 16}" y="{y0 + panel_h - 22}" text-anchor="end" fill="#555">{longest:.1f} s</text>'
        )
        parts.append(f'<text x="6" y="{y0 + 12}" fill="#555">height</text>')
    base = 2 * panel_h + 44
    for index, label in enumerate(("broken", "fixed")):
        m = result[label]
        line = (
            f"{label}: mean airtime {m['mean_flight_s']:.2f} s, episode {m['mean_duration_s']:.2f} s, "
            f"success {100 * m['success_rate']:.1f}%, crash {100 * m['crash_rate']:.1f}%, "
            f"time-out {100 * m['timeout_rate']:.1f}%"
        )
        parts.append(f'<text x="{margin}" y="{base + 18 * index}">{line}</text>')
    parts.append("</svg>")
    return "\n".join(parts)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--broken", type=Path, required=True, help="the hover-broken run folder")
    parser.add_argument("--fixed", type=Path, required=True, help="the hover-fixed run folder")
    parser.add_argument("--out", type=Path, required=True, help="JSON report to write")
    parser.add_argument("--svg", type=Path, required=True, help="SVG chart to write")
    args = parser.parse_args(argv)
    # progress messages go to the console
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    result = report(args.broken, args.fixed)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    args.svg.parent.mkdir(parents=True, exist_ok=True)
    args.svg.write_text(svg_of(result) + "\n")
    for label in ("broken", "fixed"):
        m = result[label]
        log.info(
            f"{label}: success {m['success_rate']:.3f} crash {m['crash_rate']:.3f} "
            f"timeout {m['timeout_rate']:.3f} mean airtime {m['mean_flight_s']:.2f}s "
            f"episode {m['mean_duration_s']:.2f}s"
        )


if __name__ == "__main__":
    main()
