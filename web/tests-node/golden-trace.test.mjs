// SPEC §7.10: the browser build must produce bit-identical physics to the native build.
// The native test (crates/nl-sim/tests/determinism.rs) checks the same stored hash.
// Needs `npm run wasm` first; run with `npm run test:wasm`.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { loadWasm } from './load-wasm.mjs';

const goldenFile = new URL('../../crates/nl-sim/tests/golden/flat_drive.json', import.meta.url);

test('wasm golden trace matches the native golden hash', () => {
  const { Game } = loadWasm();
  const expected = JSON.parse(readFileSync(goldenFile, 'utf8'));

  assert.equal(Game.golden_trace_hash(), expected.hash);
});
