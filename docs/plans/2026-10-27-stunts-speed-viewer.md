# Plan v3: g-meter, planned stunts, roadster drive with obstacles that never crash, better training viewer

> Historical. This plan is finished; its contracts now live in `docs/contracts/`. The Gauntlet it mentions is retired.

> Approved by the owner on 2026-10-27 and committed in the contracts PR (Gate 0). The contracts in §1 are split out, one file each, under `docs/contracts/`.
> v3 folds in the owner's review: an exact survival contract, approach kept apart from outcome, conservative stunt authorization, crash-rule safeguards, AI-envelope handling, stronger tests, and one dependency graph with gates.

## Context
The owner played the latest demo (origin/main + R4/obs v2/half loop/viewer button) and asked for four things:
1. A **g-meter** that records the peak g of each stunt, replacing the blackout.
2. **Barrel rolls and front/back flips.** The player asks; a planner decides whether it is possible and only then does it.
3. **Speed:**
   - Roadster-like top speed and acceleration.
   - Obstacles must not crash a car on a clean approach at any speed below 200 km/h.
4. A **training viewer** where the AI tackles obstacles and more.

**Fixed by the owner:**
- Stunts are player-requested and planner-checked.
- The blackout is removed.
- The viewer uses the existing AI. No retraining in this plan.
- Survival rule: **never crash, may not complete** (§1).

**Explore findings behind each change:**
- **G-load:** computed in `crates/nl-sim/src/g_load.rs`. The blackout was web-only (`web/src/hud/g-blackout.ts`, since removed). Nothing records a peak.
- **Stunts:**
  - PD only aims upright (`crates/nl-control/src/pd.rs:139`). There is no planner and no landing forecast.
  - A Realistic flip is about 1.4 s and about 55 kg of air.
  - The Hybrid gives the nozzles to the AI past 120°, which would interrupt a stunt.
- **Crashes at speed:**
  - Ramp curves (radius 31–39 m) bottom out the 0.2 m suspension at 31–34 m/s.
  - Jumps are spaced for about 20 m/s.
  - Bump faces exceed the 6 m/s body-impact rule.
  - Nothing has been tested above 30 m/s.
- **The car today:** 0–100 km/h in about 5 s and a 55 m/s cap (`configs/vehicle.toml [drive]`). The observation clamps speed at 60 m/s (`crates/nl-sim/src/obs.rs:23`).
- **Viewer bug:** `crates/nl-wasm/src/crowd.rs:192` flies the Hybrid-trained flight-r4 checkpoints as the bare AI, so every row shows 0 landed.

**Roadster target** (public claims, Oct 2026; docs say "inspired by", code and UI never name it):
- **Acceleration:** the base car claims 0–60 mph in 1.9 s; 0.81 s is claimed with the thruster package. Target: **0–100 km/h in 2.0 ± 0.2 s on wheels**.
- **Top speed:** about 230 mph at the reveal. Target: **370 km/h (103 m/s)**.
- **Mass:** about 1,700 kg claimed. Keep **1,800 kg**, because the thruster and air tuning rests on it.
- Sources:
  - [Wikipedia: Roadster (second generation)](https://en.wikipedia.org/wiki/Tesla_Roadster_(second_generation))
  - [GM Authority](https://gmauthority.com/blog/2026/10/tesla-roadster-2-0-revealed-with-1500-horsepower-flying-capability-video/)
  - [Not a Tesla App](https://www.notateslaapp.com/tesla-reference/1279/e)

## 1. Contracts (frozen at Gate 0; every agent builds to these)

### 1.1 Survival contract
> A **clean entry** at any speed in **(0, 200 km/h]** must never crash the car because it went over the obstacle. Completing a gap, loop, jump or stunt may need a minimum entry speed. Below that speed the piece gives a **safe fallback**, never a crash.

- **Each piece publishes `v_complete_min_m_s`** in `configs/pieces.json`. The HUD shows "too slow to clear this gap" before entry.
- **Fallbacks by piece:**
  - **Ramp gap:** a catch pit with a sloped floor and an exit ramp, safe to drive into and out of.
  - **Half loop:** a slow car rolls back down the entry, still upright. A test checks it.
  - **Jump:** a landing zone that covers every arc from `v_complete_min` to 200 km/h, plus a flat safe run-out for shorter arcs.
  - **Wall with ramp:** the ramp leads to a top deck with a down-ramp. Not clearing the wall means stopping on the ramp, not hitting the wall face.
  - **Corkscrew and halfpipe:** too little speed rolls back.
- **Not covered:** driving straight into a wall face or a parked-car row without using its ramp. That is not a clean entry and still crashes.
- **Above 200 km/h:** no promise. The speed readout turns amber.

### 1.2 Approach is separate from outcome
- **New `crates/nl-sim/src/traversal.rs`:**
  ```text
  PieceTraversal { piece_id, entry_tick, entry_speed_m_s, approach: Approach, phase: Entering | Traversing | Airborne | Exited }
  Approach { clean: bool, heading_err_rad, lateral_offset_m, pitch_rad, roll_rad, wheels_down, reason }
  ```
- **Entry is judged in the piece's local frame.** Limits in `configs/approach.toml`:
  - heading 15°;
  - lateral: inside the piece width minus 0.5 m;
  - pitch 10° relative to the piece surface;
  - roll 10°;
  - at least 3 wheels down, for ground-entry pieces only. Flip plates entered airborne are exempt.
- **The record expires** on exit, on a timeout (piece length / entry speed + 3 s), or on contact with a collider that doesn't belong to the piece.
- **The traversal never makes the car immune.** Crash classification always uses the actual contact geometry (§1.3). The traversal record only feeds the HUD hint, test assertions and the survival metric.

### 1.3 Crash-rule safeguards (scrape vs crash)
- **A contact is a `Scrape`, not a crash, only if all of these hold:**
  - it is on the underbody (contact point below the body's lower half-height minus 0.05 m, in body frame);
  - the contact normal is within 30° of body up;
  - the normal impact speed is ≤ `scrape_max_normal_m_s`, set from R2 measurements (starting value 4 m/s);
  - the tilt is under 25°.
- **Anything faster or elsewhere still crashes.** That covers violent underbody strikes, roof, sides, the nose into a vertical face, and tumbles.
- **Every contact is logged in sweeps:** location, normal, normal speed and classification. That way "zero crashes" can't come from a weaker detector. The before and after sweeps compare impact severity as well as crash counts.

### 1.4 Ramp curvature (defined)
- **Concave entry transition:** suspension load per unit mass is `n = v²/R + g·cosθ`. Require `n ≤ n_max`, where `n_max` is the measured sustainable load with the bump stop (R2b, expected about 3 g total). So `R ≥ v²/(n_max − g·cosθ)` at v = 55.6 m/s.
- **Convex lip and crests:** `v²/R ≤ g·cosθ` is allowed to break (the car takes off on purpose), but only at the lip. Mid-piece crests must satisfy it, so a car never goes light by accident.
- **Kink limit:** the polyline kink is ≤ 1.5° per segment.

### 1.5 Stunt state machine and nozzle ownership
- **Phases:** `Idle → Requested → Authorized → SpinUp → Coast → Catch → Settled | Aborted`.
- **Exactly one owner of the nozzles each tick:** StuntDirector during SpinUp and Coast, PD from Catch on. The Hybrid gate is held to PD throughout the stunt and for 1 s after `Settled`.
- **Ownership is written into the HUD and the event stream.**

### 1.6 Events and HUD numbering (reserved)
- **HudField:** 23 `PeakGLoad`, 24 `StuntPhase`, 25 `StuntCode`, 26 `ApproachHint`, 27 `PolicyOutOfEnvelope`.
- **Events:** the Save event gets a 5th float, `peak_g`. New events: 11 StuntStart, 12 StuntLanded, 13 StuntRefused, 14 StuntAborted, 15 Scrape, 16 TooSlowFallback.

### 1.7 G-meter definitions
- **Raw peak:** the running max of the 120 Hz **unsmoothed** seat g. Displayed g is the 0.1 s smoothed value.
- **Stunt window:** from `AirborneStart` (or `StuntStart`) to 0.5 s after the touchdown that ends it. **Touchdown counts**, because landing g is part of the stunt.
- **Reset:** the peak resets at the next `AirborneStart`, a respawn or a placement.
- **Best-ever record:** `localStorage` key `ninelives.g.v1 = {bestPeakG, version}`, wrapped in try/catch. Unknown or older shapes are dropped, which is the migration path.

## 2. Dependency graph and gates (one integration owner: me, coordinating the agents)
```text
G0 Contracts ──► G1 Baseline ──► G2 Drive+geometry+crash rules ──► G3 PD retune ──► G4 Stunts ──► G5 Viewer/HUD polish + final golden trace
     │                │
     └─► R1 stunt research, R2 speed research (feed G2/G4)
     └─► A1 g-peak sim, D1 viewer Hybrid fix (no physics change; may merge any time after G0)
```
**Rules:**
- An agent does not start a gate's work until the previous gate is merged to main.
- Sim-affecting PRs re-record the golden trace with a reviewed diff.
- One **final authoritative re-record** happens at G5 after the last sim change.

### G0 `contracts`
- Commit this plan.
- `docs/contracts/` holds §1.1–1.7.
- Add the reserved enum comment in `crates/nl-wasm/src/lib.rs`.
- Add empty config stubs: `approach.toml`, `pieces.json` fields, `[stunt]`.

### G1 `baseline`
- Under current physics, record:
  - the PD/Hybrid evals A–F (both modes, plain and `--dr`);
  - the speed sweep;
  - viewer landing rates.
- **Every eval artifact gets** `physics_hash` (hash of vehicle/modes/pieces/approach configs), `controller_config_hash`, `policy_hash` and the git SHA. This goes in `python/ninelives/eval.py` and the nl-control eval example.
- Results go to `results/baseline_2026-10/`. Historical results stay separate from new-physics results.

### R1 `research-stunts` (alongside G0/G1)
- **Survey** in `docs/research/stunt-planning.md`:
  - quadrotor multi-flip primitives (Lupashin and D'Andrea);
  - minimum-time bang-bang attitude moves;
  - Rocket League aerial bots;
  - time-to-impact estimation;
  - experimental research on thruster-assisted car flight.
- **Method:** spin-up, coast, then PD catch, with replanning.
- **Measurement:** `crates/nl-control/examples/stunt_budget.rs` measures per axis and per mode:
  - the reachable α through the allocator;
  - air for spin-up, braking, the attitude catch and landing;
  - airtime per playground ramp.
- **Output:** the `[stunt]` defaults.

### R2 `research-speed` (alongside G0/G1)
- **(a) Drive values** for 0–100 km/h in 2.0 s and 103 m/s: engine force, power and longitudinal grip. Check that steering and lateral grip stay usable at 100 m/s.
- **(b) Bump-stop design and the measured `n_max`.**
- **(c) Per-piece speed sweep and cause table** in `docs/research/speed-survival.md`.

### A1 `g-peak-sim` (any time after G0)
- `EventTracker` keeps the raw peak g per §1.7, and Save carries `peak_g`.
- HudField 23.
- Events go to 5 floats.

### D1 `viewer-hybrid-stages` (any time after G0)
- `crowd.rs` flies through `build_controller("hybrid", …)`.
- `Crowd::for_stage` covers A, D, F, the gauntlet and the arena.
- `episode::course_for_stage()` lets the web draw generated courses.
- Pull the arena obstacles into the drop zone.

### G2 `roadster-drive` and `fearless-pieces` (two PRs, after G1 and R2)
**Drive**
- `[drive]` values from R2a, with `max_speed_m_s = 103`.
- Rapier **CCD** on the chassis.
- **Wheel-ground detection tested separately from CCD:** wheel ray contact at 100 m/s over seams and kinks.
- Driver bot validated to 55.6 m/s.

**Pieces**
- `traversal.rs` (§1.2), the scrape rule (§1.3) and the bump stop.
- Piece geometry per §1.4.
- Fallbacks per §1.1.
- Re-lay out `courses/playground.json`.
- Generated-course spacing in `course_gen.rs` follows the arc maths.

**Bump stop**
- Energy check: the rebound energy is ≤ the compression energy over a drop test.
- Stability check: no oscillation growth over 10 s of bump-strip driving at 55 m/s.

### G3 `pd-retune-new-physics`
- Re-tune PD with the CMA-ES tuner under the new physics, using the earlier adoption rule.
- Regenerate `results/eval_*` with hashes.
- **Policy envelope:** `PolicyOutOfEnvelope` is set when speed > 55 m/s (policy-validated) or other obs clamp. Then Hybrid falls back to PD, unless the Hybrid has been validated in that region. Validation means running the Hybrid eval there and logging the result.

### G4 `stunt-forecast`, `stunt-planner` and `stunt-keys`
- **Landing forecast:** `World::forecast_landing()` returns touchdown time, point, **surface normal**, **usable landing length** (the distance of flat-enough, obstacle-free surface around the point) and **uncertainty**. The uncertainty is a time and position spread from ±2% speed and ±1° pitch samples.
- **Planner authorizes only if all of these hold:**
  1. the landing surface normal is within 15° of up and the usable length is ≥ 2 car lengths;
  2. `t_avail − uncertainty ≥ t_spin + t_catch + t_settle_margin`;
  3. air ≥ spin-up + braking + catch + landing reserve (R1 budgets);
  4. the current angular velocity, orientation, tank state and mode authority are inside the start envelope.
- **Full rotation tracked** by integrating the body angular velocity about the stunt axis. "Complete" needs ≥ 2π − 0.15 rad, never just "upright at the end".
- **Replanned every control tick.** If the landing window shrinks below the catch time, the planner aborts to Catch (recovery first) and emits StuntAborted.
- **Keys:**
  - **Q/E** barrel roll left/right, **Z** front flip, **B** back flip;
  - a queued request expires after 3 s;
  - toasts: "Back flip! max 3.8 g", "Not enough air time", "Landing blocked".

### G5 `g-meter-web`, `approach-hud` and `viewer-curve-ui`, then the final golden trace
**G-meter web**
- Delete the blackout, and add `g-meter.ts` and `g-record.ts`.

**Approach HUD**
- Approach hint, too-slow warning, spark effect for Scrape, amber speed above 200 km/h, and an "AI outside envelope" badge.

**Viewer**
- `export.py --viewer-curve` evaluates the 8 committed checkpoints under Hybrid in the current physics. The curve is titled **"Checkpoint evaluation under current physics"**, not a historical training curve.
- Stage tabs and a leader camera.
- Reproducible **showcase seeds**, chosen and committed, for each tab.

**Docs:** SPEC §7.2, §7.9, §11.3, §11.5, §11.6 (dedupe) and a new §11.8 Stunts. A DECISIONS line for each decision.

## 3. Acceptance tests
Every PR runs: `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; `cd web && npm run wasm && npm run test:wasm`; `npm run lint && npm run build && npx playwright test`; and `ruff` + `pytest` if Python changed.

| Area | Checks |
|---|---|
| **Survival** | Clean entry on every covered piece at 2, 5, 9.9, 10, 20, 30, 40, 50, 55.5, 55.6 m/s and 55.7 m/s (logged only), 20 seeds, both modes, PD → 0 crashes. Below `v_complete_min`, the fallback is taken and is safe. |
| **Approach boundaries** | Just inside and just outside each limit (heading, lateral, pitch, roll) → `clean` flips as expected. Outside the limits, crashes happen where the geometry says they should. |
| **Contact robustness** | Obstacle positions shifted by fractions of one tick's travel (0, ¼, ½, ¾ × v·dt). Wheel ray contact over seams at 100 m/s. A CCD wall test at 100 m/s registers a crash with no tunnelling. |
| **Crash safeguards** | A violent underbody strike above `scrape_max_normal_m_s` still crashes. Gentle ramp scrapes are Scrape. Sweeps report bottoming-out frequency, impact severity histograms and trajectories before and after. |
| **Bump stop** | Energy is not gained. No oscillation growth at 55 m/s. |
| **Drive** | 0–100 km/h in 2.0 ± 0.2 s. Top speed 103 ± 2 m/s. Lateral grip usable at 100 m/s (`drive_feel`). |
| **Stunt success** | Each stunt is a fixed case: launch position, initial speed, tank state, mode and controller, e.g. playground ramp at x = 310, 22 m/s, full tank, Realistic, PD. Rotation ≥ 2π − 0.15, a stable touchdown, no crash in a 1.5 s settling window, heading error < 30°. Sign tests: front flip nose down, back flip nose up, barrel right rolls right side down. |
| **Stunt refusal** | Refused for low air, existing spin above the start envelope, a blocked landing (obstacle at the forecast point), too little braking time, or a short airtime. An abort mid-stunt when the landing shrinks → recovered landing. |
| **Ownership** | Exactly one nozzle owner each tick (asserted in the executor). Hybrid gate held throughout. PD output bit-identical with stunts off. |
| **G-meter** | Touchdown is included. Reset at the next take-off and on respawn. Raw vs displayed peak both reported. An old or unknown localStorage shape is dropped safely. |
| **Viewer quality** | Showcase seeds are reproducible across reloads. Landing rate per tab is ≥ the floor set from the G1 baseline (e.g. the Stage A best checkpoint ≥ 70%). Frame time ≤ 16 ms on the dev machine with 64 cars (perf overlay). |
| **AI envelope** | Above 55 m/s the `PolicyOutOfEnvelope` flag is set and Hybrid uses PD. Eval artifacts carry the physics, controller and policy hashes. |

**Browser check** on the running demo (Chrome DevTools screenshots in each PR):
- every playground piece taken flat out at about 190 km/h;
- the too-slow fallbacks;
- a bad-angle crash;
- each stunt and a refusal;
- the g-meter;
- each viewer tab.

## 4. Risks
- **PD/Hybrid scores may shift under the new physics.** G3 re-tunes PD; the game already flies PD alone. R4 is only trusted inside its envelope.
- **Raycast-vehicle stability at 103 m/s** with dt = 1/120: R2a reports it. The fallback cap is an owner decision.
- **Realistic air on long 200 km/h jumps.** The planner budgets landing air first, then refuses stunts.
- **The playground grows** because of longer ramps, catch pits and landing zones. Bounds and arena tests are updated.
- **Branding:** no roadster or company names in code, assets or UI.
