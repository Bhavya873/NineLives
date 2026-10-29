# Contract: stunt state machine and nozzle ownership

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.5). Built in G4 (`claude/stunt-forecast`, `claude/stunt-planner`, `claude/stunt-keys`).

## Phases
```text
Idle → Requested → Authorized → SpinUp → Coast → Catch → Settled
                                    \________\_______\→ Aborted
```
- **Idle:** no stunt.
- **Requested:** the player pressed a stunt key (Q/E barrel roll left/right, Z front flip, B back flip). A request not authorized within 3 s expires.
- **Authorized:** the planner checked the landing forecast, time and air, and said yes. Otherwise it emits `StuntRefused` and returns to Idle.
- **SpinUp:** nozzles push to build the spin.
- **Coast:** the car rotates on its own momentum.
- **Catch:** PD stops the spin and lands the car upright.
- **Settled:** on the wheels, stable. Emits `StuntLanded`.
- **Aborted:** the planner, replanning every control tick, saw the landing window shrink below the catch time. It hands over to Catch (recovery first) and emits `StuntAborted`.

A stunt is **complete** only if the integrated rotation about the stunt axis is ≥ 2π − 0.15 rad. Being upright at the end is not enough.

## Who owns the nozzles
Exactly **one** owner each tick:

| Phase | Owner |
|---|---|
| SpinUp, Coast | StuntDirector |
| Catch, Settled, Aborted (recovery) | PD |

The Hybrid gate is held to PD for the whole stunt and for **1 s** after `Settled`, so the AI never takes over mid-stunt. Ownership is written to the HUD (`StuntPhase`) and to the event stream (see `events-hud.md`).

## Planned `[stunt]` settings
The plan asks for a `[stunt]` table. It is **not added yet**: `configs/controllers.toml` is parsed strictly (`deny_unknown_fields` in `crates/nl-control/src/config.rs`), so an unknown table would break loading. The G4 planner PR adds the table and its parser together, with values from R1 (`claude/research-stunts`). Planned entries, all *(start)*:
- `request_timeout_s = 3.0`: how long a queued request waits.
- `hybrid_hold_after_settled_s = 1.0`: how long the Hybrid gate stays on PD after Settled.
- `complete_rotation_slack_rad = 0.15`: rotation short of 2π still counted complete.
- `landing_max_normal_tilt_deg = 15.0`, `landing_min_length_car_lengths = 2.0`: landing surface checks.
- spin-up, braking, catch and landing-reserve air budgets per axis and mode (from R1).

## Tests that pin this (plan §3)
- **Ownership:** exactly one nozzle owner each tick (asserted in the executor). Hybrid gate held throughout. PD output bit-identical with stunts off.
- **Stunt success:** fixed cases (e.g. playground ramp at x = 310, 22 m/s, full tank, Realistic, PD): rotation ≥ 2π − 0.15, stable touchdown, no crash in a 1.5 s settling window, heading error < 30°.
- **Sign tests:** front flip nose down, back flip nose up, barrel right rolls right side down.
- **Stunt refusal:** refused for low air, existing spin above the start envelope, a blocked landing, too little braking time, or a short airtime. An abort mid-stunt when the landing shrinks ends in a recovered landing.
