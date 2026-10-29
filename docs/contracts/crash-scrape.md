# Contract: scrape or crash (crash-rule safeguards)

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.3). Built in G2 (`claude/fearless-pieces`).

## Why
At high speed the car's underbody touches ramps and crests. A gentle touch like that should not cost a life. But the rule must not be loosened so far that real crashes stop counting.

## The rule
A contact is a **Scrape** (not a crash) only if **all four** hold:
1. **Underbody:** the contact point is below the body's lower half-height minus 0.05 m, in the body frame.
2. **Normal points up:** the contact normal is within 30° of body up.
3. **Gentle:** the normal impact speed is ≤ `scrape_max_normal_m_s`. Starting value **4 m/s** *(start)*; R2 measurements set the final value.
4. **Nearly level:** the tilt is under 25°.

Anything else is judged by the existing crash rules and still crashes. That covers:
- violent underbody strikes (faster than `scrape_max_normal_m_s`);
- roof and side hits;
- the nose into a vertical face;
- tumbles.

A Scrape emits event 15 `Scrape` (see `events-hud.md`) and the web shows a spark effect.

## Every contact is logged in sweeps
Each contact records: location, normal, normal speed and classification (Scrape or crash). "Zero crashes" must not come from a weaker detector, so the before and after sweeps compare **impact severity** as well as crash counts.

## Where the numbers go
`scrape_max_normal_m_s` and the 0.05 m, 30° and 25° limits are tunable and belong in a config, not in code (CLAUDE.md rule 6). The G2 PR picks the file. `configs/vehicle.toml` is parsed strictly (`deny_unknown_fields` in `crates/nl-sim/src/vehicle_config.rs`), so the field and its parser land together.

## Tests that pin this (plan §3)
- **Crash safeguards:** a violent underbody strike above `scrape_max_normal_m_s` still crashes. Gentle ramp scrapes are Scrape.
- Sweeps report bottoming-out frequency, impact severity histograms and trajectories, before and after.
- **Contact robustness:** a CCD wall test at 100 m/s registers a crash with no tunnelling.
- **Browser check:** a bad-angle crash, screenshotted in the PR.
