// Unit tests for the thruster visuals (SPEC §11.4, §11.5): flames follow the throttles,
// the nozzle map places dots by nozzle position, and the air gauge text. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Group } from 'three';

import { airGaugeText } from '../src/hud/air-gauge.ts';
import { mapPercent } from '../src/hud/nozzle-map.ts';
import { readNozzleLayout } from '../src/nozzle-layout.ts';
import { createFlames, updateFlames } from '../src/render/flames.ts';
import { Game, modesToml, vehicleToml } from './load-wasm.mjs';

function nozzleLayout() {
  return new Game(vehicleToml, modesToml, 0n).nozzle_layout();
}

test('the nozzle layout has 10 nozzles and lift exhaust points down', () => {
  const layout = nozzleLayout();
  const nozzles = readNozzleLayout(layout);

  assert.equal(layout.length, Game.nozzle_count() * Game.floats_per_nozzle());
  assert.equal(nozzles.length, 10);
  // L_FL: front-left, below the centre of mass; it pushes up, so its gas goes down.
  assert.ok(nozzles[0].positionM[0] > 0, 'L_FL is at the front');
  assert.ok(nozzles[0].positionM[2] < 0, 'L_FL is on the left');
  assert.deepEqual(nozzles[0].exhaust, [-0, -1, -0]);
});

test('a flame shows only when its nozzle fires, and grows with the throttle', () => {
  const carBody = new Group();
  const flames = createFlames(carBody, readNozzleLayout(nozzleLayout()));
  const throttles = new Float32Array(10);
  throttles[0] = 0.5;
  throttles[9] = 1;

  updateFlames(flames, throttles);

  assert.equal(flames.length, 10);
  assert.equal(flames[0].visible, true);
  assert.equal(flames[1].visible, false);
  assert.ok(flames[9].scale.y > flames[0].scale.y);
  // Each nozzle is a cone on the car body with its flame as a child.
  assert.equal(carBody.children.length, 10);
  assert.equal(flames[0].parent?.parent, carBody);
});

test('the nozzle map spans the car from edge to edge', () => {
  assert.equal(mapPercent(-1.3, 1.3), 0);
  assert.equal(mapPercent(0, 1.3), 50);
  assert.equal(mapPercent(1.3, 1.3), 100);
});

test('the air gauge shows whole percent, or ∞ with unlimited air', () => {
  assert.equal(airGaugeText(0.456, false), '46%');
  assert.equal(airGaugeText(0.1, true), '∞');
});
