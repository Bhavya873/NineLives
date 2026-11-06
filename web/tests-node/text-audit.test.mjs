// Text audit: lists every visible string of the game page, the viewer page and the UI modules,
// and fails when a sentence is over six words, a toast has "!" or a g number, or a toast is long.
// Plain Node. Strings built from live numbers (speeds, counts) are checked with a sample number.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import {
  STUNT_NAME,
  stuntToastForEvents,
  StuntEvent,
  RefusedReason,
} from '../src/game/stunt-requests.ts';
import { ARENAS, arenaLabel } from '../src/viewer/arena-choice.ts';
import { approachHintText, tooSlowWarning } from '../src/ui/approach-hud.ts';
import { HOP_BLOCKED_TEXT, MODE_TEXT } from '../src/ui/hud.ts';
import { BACK_IN_BOUNDS_TEXT, NO_AIR_TEXT, toastForEvents } from '../src/ui/toasts.ts';
import { CURVE_LABEL } from '../src/viewer/curve.ts';
import { CrashCode, EventKind } from './load-wasm.mjs';

const MAX_WORDS = 6;

/** The visible text of a page: tags, comments and scripts stripped, plus aria-labels. */
function pageStrings(file) {
  const html = readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');
  const labels = [...html.matchAll(/aria-label="([^"]+)"/g)].map((match) => match[1]);
  const text = html
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<(script|style)[\s\S]*?<\/\1>/g, ' ')
    .replace(/<svg[\s\S]*?<\/svg>/g, ' ')
    .replace(/<[^>]*>/g, '\n')
    .split('\n')
    .map((line) => line.replace(/\s+/g, ' ').trim())
    .filter(Boolean);
  return [...text, ...labels];
}

/** The strings the UI modules can show. */
function moduleStrings() {
  const crash = (code) => toastForEvents(new Float32Array([EventKind.Crash, code, 0, 0, 0]));
  const stunt = (kind, code, reason) =>
    stuntToastForEvents(new Float32Array([kind, code, reason, 0, 0]));
  return [
    toastForEvents(new Float32Array([EventKind.Save, 1, 14, 3, 3])),
    toastForEvents(new Float32Array([EventKind.CheckpointReached, 0, 0, 0, 0])),
    toastForEvents(new Float32Array([EventKind.Finished, 0, 0, 0, 0])),
    ...Object.values(CrashCode)
      .filter((code) => typeof code === 'number')
      .map(crash),
    BACK_IN_BOUNDS_TEXT,
    NO_AIR_TEXT,
    ...Object.values(STUNT_NAME),
    stunt(StuntEvent.Refused, 3, RefusedReason.NotEnoughAir),
    stunt(StuntEvent.Refused, 3, RefusedReason.LandingBlocked),
    stunt(StuntEvent.Aborted, 3, RefusedReason.ControlLost),
    ...Object.values(HOP_BLOCKED_TEXT),
    ...Object.values(MODE_TEXT),
    'hop ready in 1 s',
    approachHintText(60),
    tooSlowWarning(new Float32Array([16, 1, 10, 0, 0])),
    'respawning',
    ...ARENAS.map(arenaLabel),
    CURVE_LABEL,
    '8 checkpoints, 5 stages',
    '12/16 landed',
    'camera: crowd',
    'camera: leader',
    'Out of lives - 12 saves',
  ].filter((text) => typeof text === 'string' && text !== '');
}

/** Words in a sentence: tokens with a letter or digit ("/" and "-" do not count). */
function wordCount(sentence) {
  return sentence.split(/\s+/).filter((token) => /[\p{L}\p{N}]/u.test(token)).length;
}

/** A string cut into sentences. */
function sentences(text) {
  return text.split(/(?<=[.!?])\s+/).filter(Boolean);
}

const all = [...pageStrings('index.html'), ...pageStrings('viewer.html'), ...moduleStrings()];

test('every visible string, listed', (t) => {
  t.diagnostic(`${all.length} strings:\n${all.map((text) => `  ${text}`).join('\n')}`);
  assert.ok(all.length > 40);
});

test(`no sentence is over ${MAX_WORDS} words`, () => {
  const long = all.flatMap(sentences).filter((sentence) => wordCount(sentence) > MAX_WORDS);
  assert.deepEqual(long, []);
});

test('toasts have no exclamation mark and no g number', () => {
  const toasts = moduleStrings();
  for (const text of toasts) {
    assert.doesNotMatch(text, /!/, text);
    assert.doesNotMatch(text, /\d\s*g\b/, text);
    assert.doesNotMatch(text, /\bkg\b/, text);
  }
});

test('the removed help blocks and copy are gone', () => {
  const text = all.join('\n');
  assert.doesNotMatch(text, /How it works/i);
  assert.doesNotMatch(text, /Each row of cars/);
  assert.doesNotMatch(text, /Click a title/);
});
