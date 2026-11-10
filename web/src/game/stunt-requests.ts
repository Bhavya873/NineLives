// Stunt keys (A S barrel roll, D F flip): the request queue and the toast mapping (plan G4, docs/contracts/stunt-states.md).
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
  /** Too little air time for the spin, brake and catch. */
  NotEnoughTime: 4,
} as const;

/** Keys that request a stunt, by `KeyboardEvent.code`. */
export const STUNT_KEYS: Record<string, StuntCodeValue> = {
  KeyA: StuntCode.BarrelLeft,
  KeyS: StuntCode.BarrelRight,
  KeyD: StuntCode.FrontFlip,
  KeyF: StuntCode.BackFlip,
};

/** What the toast calls each stunt (left and right roll read the same). */
export const STUNT_NAME: Record<number, string> = {
  [StuntCode.BarrelLeft]: 'barrel roll',
  [StuntCode.BarrelRight]: 'barrel roll',
  [StuntCode.FrontFlip]: 'front flip',
  [StuntCode.BackFlip]: 'back flip',
};

/** One waiting request. */
interface StuntRequest {
  code: StuntCodeValue;
  /** When it was pressed, in seconds on the caller's clock. */
  requestedAtS: number;
  /** True once the adapter has passed it to the sim. */
  submitted: boolean;
}

/** Most stunts in one jump (the planner's MAX_STUNTS). */
export const MAX_STUNTS = 3;

/** Waiting requests in press order, at most MAX_STUNTS; the sim flies them back to back. */
interface StuntQueue {
  pending: StuntRequest[];
}

export function createStuntQueue(): StuntQueue {
  return { pending: [] };
}

/** A stunt key was pressed: queue it behind the others. A fourth press is ignored. */
export function pressStunt(queue: StuntQueue, code: StuntCodeValue, nowS: number): void {
  if (queue.pending.length < MAX_STUNTS) {
    queue.pending.push({ code, requestedAtS: nowS, submitted: false });
  }
}

/** Drops the requests older than the timeout. Returns true if it dropped any. */
export function expireStunt(queue: StuntQueue, nowS: number): boolean {
  const before = queue.pending.length;
  queue.pending = queue.pending.filter((r) => nowS - r.requestedAtS < STUNT_REQUEST_TIMEOUT_S);
  return queue.pending.length < before;
}

/**
 * Hands the oldest request the sim has not seen to it through `submit` (the thin adapter).
 * `submit` returns false when the sim cannot take it (planner not built yet, jump full or
 * past braking); the request then stays queued and is retried until it expires or the
 * stunt ends. One request per call keeps the press order.
 */
export function submitPendingStunt(
  queue: StuntQueue,
  submit: (code: StuntCodeValue) => boolean,
): void {
  const request = queue.pending.find((r) => !r.submitted);
  if (request) {
    request.submitted = submit(request.code);
  }
}

const COUNT_WORD: Record<number, string> = { 2: 'double', 3: 'triple' };
/** What a mixed move calls each kind of stunt. */
const CLASS_NAME: Record<number, string> = {
  [StuntCode.BarrelLeft]: 'roll',
  [StuntCode.BarrelRight]: 'roll',
  [StuntCode.FrontFlip]: 'flip',
  [StuntCode.BackFlip]: 'flip',
};

/** The stunt codes of a jump from the packed `chain` float (first stunt in the ones digit). */
export function chainCodes(chain: number, fallback: number): number[] {
  const codes: number[] = [];
  let rest = Math.round(chain);
  while (rest > 0 && codes.length < MAX_STUNTS) {
    codes.push(rest % 10);
    rest = Math.floor(rest / 10);
  }
  return codes.length > 0 && codes.every((c) => c in STUNT_NAME) ? codes : [fallback];
}

/**
 * The plain lowercase name of a jump's stunts: 'back flip', 'double back flip',
 * 'triple barrel roll', 'flip + roll'.
 */
export function stuntMoveName(codes: number[]): string {
  const first = codes[0] ?? -1;
  if (codes.length < 2) {
    return STUNT_NAME[first] ?? 'landed';
  }
  const count = COUNT_WORD[codes.length] ?? '';
  if (codes.every((c) => c === first)) {
    return `${count} ${STUNT_NAME[first]}`;
  }
  const classes = codes.map((c) => CLASS_NAME[c]);
  if (classes.every((c) => c === classes[0])) {
    return `${count} ${classes[0] === 'roll' ? 'barrel roll' : 'flip'}`;
  }
  return classes.join(' + ');
}

/** The toast for a batch of drained events (last stunt event wins), or null. */
export function stuntToastForEvents(events: Float32Array): string | null {
  let text: string | null = null;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    const kind = events[start];
    const code = events[start + 1] ?? -1;
    if (kind === StuntEvent.Landed) {
      // [Landed, stunt_code, peak_g, rotation_rad, chain]; the g number stays out of the toast
      text = stuntMoveName(chainCodes(events[start + 4] ?? 0, code));
    } else if (kind === StuntEvent.Refused) {
      // [Refused, stunt_code, reason]
      const reason = events[start + 2];
      if (reason === RefusedReason.LandingBlocked) {
        text = 'landing blocked';
      } else if (reason === RefusedReason.NotEnoughTime) {
        text = 'not enough time';
      } else if (reason !== RefusedReason.Expired) {
        text = 'not enough air';
      }
    } else if (kind === StuntEvent.Aborted) {
      // [Aborted, stunt_code, reason, rotation_rad, chain]
      text =
        events[start + 2] === RefusedReason.LandingBlocked ? 'landing blocked' : 'stunt aborted';
    }
  }
  return text;
}

/**
 * Settles the queue with what the sim answered, in order: every `StuntStart` (taken) and
 * `StuntRefused` (turned down) answers the oldest request the sim has seen; a landing or an
 * abort ends the jump's stunts, so whatever still waits is dropped.
 */
export function applyStuntEvents(queue: StuntQueue, events: Float32Array): void {
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    const kind = events[start];
    if (kind === StuntEvent.Start || kind === StuntEvent.Refused) {
      const index = queue.pending.findIndex((r) => r.submitted);
      if (index >= 0) {
        queue.pending.splice(index, 1);
      }
    } else if (kind === StuntEvent.Landed || kind === StuntEvent.Aborted) {
      queue.pending = [];
    }
  }
}
