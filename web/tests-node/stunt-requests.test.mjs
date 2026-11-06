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
  assert.deepEqual(queue.pending, []);
});

test('a newer press queues behind the older one, up to three', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BarrelLeft, 0);
  pressStunt(queue, StuntCode.BarrelRight, 1);
  pressStunt(queue, StuntCode.BarrelRight, 1.5);
  pressStunt(queue, StuntCode.BackFlip, 2);
  assert.deepEqual(
    queue.pending.map((r) => r.code),
    [StuntCode.BarrelLeft, StuntCode.BarrelRight, StuntCode.BarrelRight],
    'a fourth press is ignored',
  );
  assert.equal(queue.pending[1].requestedAtS, 1);
});

test('requests go to the sim one per call, in press order', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BackFlip, 0);
  pressStunt(queue, StuntCode.BarrelRight, 0.1);
  const seen = [];
  const take = (code) => (seen.push(code), true);
  submitPendingStunt(queue, take);
  submitPendingStunt(queue, take);
  submitPendingStunt(queue, take);
  assert.deepEqual(seen, [StuntCode.BackFlip, StuntCode.BarrelRight]);
});

test('a refused submit holds the line: the older request goes first again', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BackFlip, 0);
  pressStunt(queue, StuntCode.FrontFlip, 0.1);
  const seen = [];
  submitPendingStunt(queue, (code) => (seen.push(code), false));
  submitPendingStunt(queue, (code) => (seen.push(code), false));
  assert.deepEqual(seen, [StuntCode.BackFlip, StuntCode.BackFlip]);
});

test('only the requests past the timeout lapse', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BackFlip, 0);
  pressStunt(queue, StuntCode.BackFlip, 2);
  assert.equal(expireStunt(queue, STUNT_REQUEST_TIMEOUT_S), true);
  assert.equal(queue.pending.length, 1);
});

test('start and refused events resolve the request', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BackFlip, 0);
  queue.pending[0].submitted = true;
  applyStuntEvents(queue, ev(StuntEvent.Start, 3));
  assert.deepEqual(queue.pending, []);
  pressStunt(queue, StuntCode.BackFlip, 0);
  queue.pending[0].submitted = true;
  applyStuntEvents(queue, ev(StuntEvent.Refused, 3, 0));
  assert.deepEqual(queue.pending, []);
});

test('each answer settles one submitted request; a landing drops the rest', () => {
  const queue = createStuntQueue();
  pressStunt(queue, StuntCode.BackFlip, 0);
  pressStunt(queue, StuntCode.BackFlip, 0.1);
  pressStunt(queue, StuntCode.BarrelLeft, 0.2);
  submitPendingStunt(queue, () => true);
  submitPendingStunt(queue, () => true);
  const two = new Float32Array(10);
  two.set(ev(StuntEvent.Start, 4), 0);
  two.set(ev(StuntEvent.Refused, 4, RefusedReason.NotEnoughTime), 5);
  applyStuntEvents(queue, two);
  assert.deepEqual(
    queue.pending.map((r) => [r.code, r.submitted]),
    [[StuntCode.BarrelLeft, false]],
  );
  applyStuntEvents(queue, ev(StuntEvent.Landed, 4, 3.8, 6.3, 44));
  assert.deepEqual(queue.pending, [], 'a landing ends the jump');
  pressStunt(queue, StuntCode.FrontFlip, 5);
  applyStuntEvents(queue, ev(StuntEvent.Aborted, 3, RefusedReason.ControlLost));
  assert.deepEqual(queue.pending, [], 'an abort too');
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
  assert.equal(
    stuntToastForEvents(ev(StuntEvent.Refused, 3, RefusedReason.NotEnoughTime)),
    'not enough time',
  );
  assert.equal(stuntToastForEvents(ev(StuntEvent.Start, 3)), null);
  assert.equal(stuntToastForEvents(new Float32Array(0)), null);
});

test('the toast names the whole move, in plain lowercase words', () => {
  const landed = (code, chain) => stuntToastForEvents(ev(StuntEvent.Landed, code, 3.8, 6.3, chain));
  assert.equal(landed(4, 44), 'double back flip');
  assert.equal(landed(3, 333), 'triple front flip');
  assert.equal(landed(2, 222), 'triple barrel roll');
  assert.equal(landed(1, 11), 'double barrel roll');
  assert.equal(landed(1, 21), 'double barrel roll', 'left then right is still a double roll');
  assert.equal(landed(4, 34), 'double flip');
  assert.equal(landed(4, 24), 'flip + roll', 'a back flip then a barrel roll right');
  assert.equal(landed(2, 342), 'roll + flip + flip');
  assert.equal(landed(4, 0), 'back flip', 'an old build sends no chain');
  assert.equal(landed(2, 2), 'barrel roll');
  for (const chain of [44, 333, 222, 24, 342, 21]) {
    const text = landed(4, chain);
    assert.ok(text.split(/\s+/).filter((w) => /\w/.test(w)).length <= 6, text);
    assert.doesNotMatch(text, /!|\d/);
  }
});

test('the adapter returns false while the sim has no stunt method, and forwards once it does', () => {
  assert.equal(requestStuntInSim({}, StuntCode.BackFlip), false);
  const seen = [];
  const game = { request_stunt: (code) => (seen.push(code), true) };
  assert.equal(requestStuntInSim(game, StuntCode.BackFlip), true);
  assert.deepEqual(seen, [StuntCode.BackFlip]);
});
