// Unit tests for the stunt key queue and toast mapping. Plain Node, no wasm.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { requestStuntInSim } from '../src/wasm-bridge/stunt-adapter.ts';
import {
  STUNT_KEYS,
  STUNT_REQUEST_TIMEOUT_S,
  RefusedReason,
  StuntCode,
  StuntEvent,
  applyStuntEvents,
  createStuntQueue,
  expireStunt,
  pressStunt,
  stuntToastForEvents,
  submitPendingStunt,
} from '../src/game/stunt-requests.ts';

const ev = (...values) => {
  const out = new Float32Array(5);
  out.set(values);
  return out;
};

test('keys map to the four stunts', () => {
  assert.deepEqual(STUNT_KEYS, {
    KeyA: StuntCode.BarrelLeft,
    KeyS: StuntCode.BarrelRight,
    KeyD: StuntCode.FrontFlip,
    KeyF: StuntCode.BackFlip,
  });
});

test('event kinds match the frozen contract numbers', () => {
  assert.deepEqual(StuntEvent, { Start: 11, Landed: 12, Refused: 13, Aborted: 14 });
});

test('a press only queues; nothing reaches the sim until submit', () => {
  const queue = createStuntQueue();
  let calls = 0;
  pressStunt(queue, StuntCode.BackFlip, 10);
  assert.equal(calls, 0);
  submitPendingStunt(queue, () => {
    calls += 1;
    return true;
  });
  submitPendingStunt(queue, () => {
    calls += 1;
    return true;
  });
  assert.equal(calls, 1, 'submitted once');
});

test('a refused submit is retried until the request expires at 3 s', () => {
  const queue = createStuntQueue();
  let calls = 0;
  pressStunt(queue, StuntCode.FrontFlip, 5);
  const stub = () => {
    calls += 1;
    return false;
  };
  submitPendingStunt(queue, stub);
  submitPendingStunt(queue, stub);
  assert.equal(calls, 2);
  assert.equal(expireStunt(queue, 5 + STUNT_REQUEST_TIMEOUT_S - 0.01), false);
  assert.equal(expireStunt(queue, 5 + STUNT_REQUEST_TIMEOUT_S), true);
  assert.equal(queue.pending, null);
});

test('a newer press replaces the pending request', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BarrelLeft, 0);
  pressStunt(queue, StuntCode.BarrelRight, 1);
  assert.equal(queue.pending.code, StuntCode.BarrelRight);
  assert.equal(queue.pending.requestedAtS, 1);
});

test('start and refused events resolve the request', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BackFlip, 0);
  applyStuntEvents(queue, ev(StuntEvent.Landed, 3, 3.8));
  assert.notEqual(queue.pending, null);
  applyStuntEvents(queue, ev(StuntEvent.Start, 3));
  assert.equal(queue.pending, null);
  pressStunt(queue, StuntCode.BackFlip, 0);
  applyStuntEvents(queue, ev(StuntEvent.Refused, 3, 0));
  assert.equal(queue.pending, null);
});

test('toasts are short plain words with no exclamation mark or g number', () => {
  assert.equal(stuntToastForEvents(ev(StuntEvent.Landed, StuntCode.BackFlip, 3.84)), 'back flip');
  assert.equal(stuntToastForEvents(ev(StuntEvent.Landed, StuntCode.FrontFlip, 5.0)), 'front flip');
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Refused, 3, RefusedReason.NotEnoughAir)),
    'not enough air',
  );
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Refused, 3, RefusedReason.LandingBlocked)),
    'landing blocked',
  );
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Refused, 3, RefusedReason.Expired)),
    null,
    'a lapsed request is silent',
  );
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Aborted, 3, RefusedReason.ControlLost)),
    'stunt aborted',
  );
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Aborted, 3, RefusedReason.LandingBlocked)),
    'landing blocked',
  );
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Landed, StuntCode.BarrelLeft, 2.0)),
    'barrel roll',
  );
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Landed, StuntCode.BarrelRight, 2.0)),
    'barrel roll',
  );
  assert.equal(stuntToastForEvents(ev(StuntEvent.Start, 3)), null);
  assert.equal(stuntToastForEvents(new Float32Array(0)), null);
});

test('the adapter returns false while the sim has no stunt method, and forwards once it does', () => {
  assert.equal(requestStuntInSim({}, StuntCode.BackFlip), false);
  const seen = [];
  const game = { request_stunt: (code) => (seen.push(code), true) };
  assert.equal(requestStuntInSim(game, StuntCode.BackFlip), true);
  assert.deepEqual(seen, [StuntCode.BackFlip]);
});
