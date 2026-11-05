# docs

Start with `../SPEC.md` (what to build) and `DECISIONS.md` (why it changed). Everything else is below.

## Core
- `DECISIONS.md`: one row per decision, newest at the bottom. A change to the golden trace needs a row here.
- `EXPERIMENTS.md`: every training run and eval round, with the config, seed and result files in `../results/`.
- `TECHNICAL.md`: how the system is built and where the code lives.
- `TRAINING.md`: how to train, resume and inspect a policy on your own machine.
- `AI-IN-PLAIN-WORDS.md`: the training pipeline explained without jargon.

## Contracts (`contracts/`)
Rules frozen before the stunts/speed/viewer work. Code comments point at them.
- `survival.md`: a clean entry never crashes up to 200 km/h. `traversal.md`, `crash-scrape.md`, `ramp-curvature.md`: how pieces are entered, scraped and shaped.
- `stunt-states.md`: planned stunt states. `g-meter.md`: peak g per stunt. `events-hud.md`: event and HUD fields.

## Decisions with long reasons (`adr/`)
- `001-one-deterministic-physics-core.md`, `002-pyo3-batched-sim.md`, `003-resumable-chunked-training.md`.

## Research (`research/`)
- `speed-survival.md`: drive values and what makes a car crash at speed.
- `stunt-planning.md`: planned flips and measured air budgets.
- `control-upgrade-audit.md`: what the thruster controller sees and drives, before the control upgrade.

## History (finished, kept for the record)
- `BUILD-PLAN.md`: the original M0 to M8 build plan. The Gauntlet in it is retired.
- `plans/2026-10-27-stunts-speed-viewer.md`: the plan behind the contracts.

## Images (`images/`)
Only images the README, SPEC or a chart script uses: `game.png`, `viewer.png`, `viewer-curve.svg`, `results-by-stage.svg`, `hover-hack.svg`.
