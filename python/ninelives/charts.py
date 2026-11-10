"""Draws the README charts (SVG, no plotting library) from the committed results.

    python -m ninelives.charts --repo . --out docs/images

Writes `results-by-stage.svg` (success rate per stage, Off / PD / Hybrid, both modes, from
`results/eval_*.json`) and `viewer-curve.svg` (the training viewer's checkpoint curve, from
`curve.json` of the newest run in `web/public/checkpoints/`).
"""

import argparse
import json
import logging
from pathlib import Path

STAGES = "ABCDE"
CONTROLLERS = [
    ("off", "Off", "#9aa5b1"),
    ("pd", "PD", "#2e86ab"),
    ("hybrid", "Hybrid (PD + AI)", "#e4572e"),
]
MODES = ["realistic", "arcade"]

log = logging.getLogger(__name__)


def success_rate(results: Path, controller: str, stage: str, mode: str) -> float | None:
    """Success rate of one eval file, or None when that controller was not run there."""
    path = results / f"eval_{controller}_{stage}_{mode}.json"
    if not path.is_file():
        return None
    return float(json.loads(path.read_text())["summary"]["success_rate"])


def by_stage_svg(results: Path) -> str:
    """Grouped bars: one panel per mode, one group per stage, one bar per controller."""
    panel_w, height, top, bottom, left = 360, 260, 40, 60, 40
    width = 2 * panel_w + left + 20
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height + 40}" '
        'font-family="system-ui, sans-serif" font-size="11">',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
    ]
    plot_h = height - top - bottom
    for mode_index, mode in enumerate(MODES):
        x0 = left + mode_index * (panel_w + 10)
        parts.append(
            f'<text x="{x0}" y="22" font-weight="600">{mode.capitalize()}, 1000 episodes per bar</text>'
        )
        for tick in (0, 0.5, 1.0):
            y = top + plot_h * (1 - tick)
            parts.append(
                f'<line x1="{x0}" x2="{x0 + panel_w - 20}" y1="{y:.1f}" y2="{y:.1f}" stroke="#ddd"/>'
            )
            if mode_index == 0:
                parts.append(
                    f'<text x="{x0 - 6}" y="{y + 4:.1f}" text-anchor="end" fill="#555">{int(tick * 100)}%</text>'
                )
        group_w = (panel_w - 20) / len(STAGES)
        for stage_index, stage in enumerate(STAGES):
            gx = x0 + stage_index * group_w
            parts.append(
                f'<text x="{gx + group_w / 2:.1f}" y="{top + plot_h + 16}" text-anchor="middle">Stage {stage}</text>'
            )
            bars = [
                (name, color, success_rate(results, key, stage, mode))
                for key, name, color in CONTROLLERS
            ]
            bar_w = (group_w - 8) / len(CONTROLLERS)
            for bar_index, (_, color, rate) in enumerate(bars):
                if rate is None:
                    continue
                h = plot_h * rate
                parts.append(
                    f'<rect x="{gx + 4 + bar_index * bar_w:.1f}" y="{top + plot_h - h:.1f}" '
                    f'width="{bar_w - 1:.1f}" height="{h:.1f}" fill="{color}"/>'
                )
    legend_y = height + 20
    for index, (_, name, color) in enumerate(CONTROLLERS):
        lx = left + index * 160
        parts.append(f'<rect x="{lx}" y="{legend_y - 9}" width="10" height="10" fill="{color}"/>')
        parts.append(f'<text x="{lx + 14}" y="{legend_y}">{name}</text>')
    parts.append("</svg>")
    return "\n".join(parts)


def curve_svg(curve_json: Path) -> str:
    """Success rate against training steps for each stage, from the viewer's curve file."""
    curve = json.loads(curve_json.read_text())
    checkpoints = curve["checkpoints"]
    stages = curve["stages"]
    palette = ["#2e86ab", "#76b041", "#e4572e", "#f6ae2d", "#6f4e7c", "#555555"]
    width, height, left, top, bottom = 560, 280, 44, 36, 44
    plot_w, plot_h = width - left - 20, height - top - bottom
    max_steps = max(entry["steps"] for entry in checkpoints)
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
        'font-family="system-ui, sans-serif" font-size="11">',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
        f'<text x="{left}" y="20" font-weight="600">{curve["title"]} ({curve["episodes"]} episodes per point)</text>',
    ]
    for tick in (0, 0.5, 1.0):
        y = top + plot_h * (1 - tick)
        parts.append(
            f'<line x1="{left}" x2="{left + plot_w}" y1="{y:.1f}" y2="{y:.1f}" stroke="#ddd"/>'
        )
        parts.append(
            f'<text x="{left - 6}" y="{y + 4:.1f}" text-anchor="end" fill="#555">{int(tick * 100)}%</text>'
        )
    for index, stage in enumerate(stages):
        points = [
            (
                left + plot_w * entry["steps"] / max_steps,
                top + plot_h * (1 - entry["stages"][stage]["success_rate"]),
            )
            for entry in checkpoints
            if stage in entry["stages"]
        ]
        text = " ".join(f"{x:.1f},{y:.1f}" for x, y in points)
        color = palette[index % len(palette)]
        parts.append(f'<polyline points="{text}" fill="none" stroke="{color}" stroke-width="1.8"/>')
        parts.append(
            f'<text x="{left + index * 60}" y="{height - 10}" fill="{color}">Stage {stage}</text>'
        )
    parts.append(
        f'<text x="{left + plot_w}" y="{height - 26}" text-anchor="end" fill="#555">'
        f"training steps (max {max_steps / 1e6:.0f}M)</text>"
    )
    parts.append("</svg>")
    return "\n".join(parts)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", type=Path, default=Path("."), help="repo root")
    parser.add_argument("--out", type=Path, default=Path("docs/images"), help="folder for the SVGs")
    args = parser.parse_args(argv)
    # progress messages go to the console
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    args.out.mkdir(parents=True, exist_ok=True)
    # chart 1: success rate per stage
    by_stage_path = args.out / "results-by-stage.svg"
    by_stage_path.write_text(by_stage_svg(args.repo / "results") + "\n")
    log.info(f"wrote {by_stage_path}")
    # chart 2: the viewer's training curve
    checkpoints = args.repo / "web" / "public" / "checkpoints"
    newest_run = json.loads((checkpoints / "index.json").read_text())["runs"][-1]["run_id"]
    curve = checkpoints / newest_run / "curve.json"
    curve_path = args.out / "viewer-curve.svg"
    curve_path.write_text(curve_svg(curve) + "\n")
    log.info(f"wrote {curve_path}")


if __name__ == "__main__":
    main()
