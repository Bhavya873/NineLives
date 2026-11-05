// Stunt keys: the request queue and the toast mapping (plan G4, docs/contracts/stunt-states.md).
// A key press is only a REQUEST. The Rust planner decides (authorizes or refuses) and tells
// us through events; this module never starts a stunt itself. No wasm import on purpose, so
// it runs in plain Node and builds before the planner lands. Event kinds are the numbers
// frozen in docs/contracts/events-hud.md.
import { FLOATS_PER_EVENT } from '../wasm-bridge/sim-events.ts';

/** A queued request that is not resolved within this many seconds is dropped. */
export const STUNT_REQUEST_TIMEOUT_S = 3;

/** Stunt event kinds (events-hud.md). */
export const StuntEvent = {
  Start: 11,
  Landed: 12,
  Refused: 13,
  Aborted: 14,
} as const;

/**
 * Which stunt: the Rust planner's `StuntKind` numbers, the same as HUD field 25
 * (`StuntCode`) and float 1 of every stunt event. 0 means no stunt.
 */
export const StuntCode = {
  BarrelLeft: 1,
  BarrelRight: 2,
  FrontFlip: 3,
  BackFlip: 4,
} as const;
export type StuntCodeValue = (typeof StuntCode)[keyof typeof StuntCode];

/** Why the planner refused or aborted (float 2 of `StuntRefused` and `StuntAborted`). */
export const RefusedReason = {
  NotEnoughAir: 0,
  LandingBlocked: 1,
  /** The request waited its 3 s without a take-off: no toast, the key just lapsed. */
  Expired: 2,
  /** Aborted in the air: thrust ran out or a wheel touched early. */
  ControlLost: 3,
} as const;

/** Keys that request a stunt, by `KeyboardEvent.code`. */
export const STUNT_KEYS: Record<string, StuntCodeValue> = {
  KeyQ: StuntCode.BarrelLeft,
  KeyE: StuntCode.BarrelRight,
  KeyZ: StuntCode.FrontFlip,
  KeyB: StuntCode.BackFlip,
};

export const STUNT_NAME: Record<number, string> = {
  [StuntCode.BarrelLeft]: 'Barrel roll left',
  [StuntCode.BarrelRight]: 'Barrel roll right',
  [StuntCode.FrontFlip]: 'Front flip',
  [StuntCode.BackFlip]: 'Back flip',
};

/** One waiting request. */
interface StuntRequest {
  code: StuntCodeValue;
  /** When it was pressed, in seconds on the caller's clock. */
  requestedAtS: number;
  /** True once the adapter has passed it to the sim. */
  submitted: boolean;
}

/** At most one request waits; a newer press replaces it. */
interface StuntQueue {
  pending: StuntRequest | null;
}

export function createStuntQueue(): StuntQueue {
  return { pending: null };
}

/** A stunt key was pressed: queue it (replacing any older request). */
export function pressStunt(queue: StuntQueue, code: StuntCodeValue, nowS: number): void {
  queue.pending = { code, requestedAtS: nowS, submitted: false };
}

/** Drops the pending request once it is older than the timeout. Returns true if it did. */
export function expireStunt(queue: StuntQueue, nowS: number): boolean {
  if (queue.pending && nowS - queue.pending.requestedAtS >= STUNT_REQUEST_TIMEOUT_S) {
    queue.pending = null;
    return true;
  }
  return false;
}

/**
 * Hands the pending request to the sim through `submit` (the thin adapter) if it has not
 * been handed over yet. `submit` returns false when the sim cannot take it (planner not
 * built yet, or busy); the request then stays queued and is retried until it expires.
 */
export function submitPendingStunt(
  queue: StuntQueue,
  submit: (code: StuntCodeValue) => boolean,
): void {
  const request = queue.pending;
  if (request && !request.submitted) {
    request.submitted = submit(request.code);
  }
}

/** The toast for a batch of drained events (last stunt event wins), or null. */
export function stuntToastForEvents(events: Float32Array): string | null {
  let text: string | null = null;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    const kind = events[start];
    const code = events[start + 1] ?? -1;
    if (kind === StuntEvent.Landed) {
      // [Landed, stunt_code, peak_g, rotation_rad]
      const name = STUNT_NAME[code] ?? 'Stunt';
      text = `${name}! max ${(events[start + 2] ?? 0).toFixed(1)} g`;
    } else if (kind === StuntEvent.Refused) {
      // [Refused, stunt_code, reason]
      const reason = events[start + 2];
      if (reason === RefusedReason.LandingBlocked) {
        text = 'Landing blocked';
      } else if (reason !== RefusedReason.Expired) {
        text = 'Not enough air time';
      }
    } else if (kind === StuntEvent.Aborted) {
      // [Aborted, stunt_code, reason, rotation_rad]
      text =
        events[start + 2] === RefusedReason.LandingBlocked ? 'Landing blocked' : 'Stunt aborted';
    }
  }
  return text;
}

/** True if the batch resolves the pending request (authorized or refused). */
function resolvesRequest(events: Float32Array): boolean {
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === StuntEvent.Start || events[start] === StuntEvent.Refused) {
      return true;
    }
  }
  return false;
}

/** Clears the pending request when the sim answered it. */
export function applyStuntEvents(queue: StuntQueue, events: Float32Array): void {
  if (resolvesRequest(events)) {
    queue.pending = null;
  }
}
