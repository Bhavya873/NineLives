// Unit tests for the thruster visuals (SPEC §11.4, §11.5): flames follow the throttles,
// ports sit on the body, the nozzle map places dots by nozzle position, and the air
// gauge text. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Box3, Group, Vector3 } from 'three';

import { airGaugeText, createAirGauge, updateAirGauge } from '../src/hud/air-gauge.ts';
import { mapPercent } from '../src/hud/nozzle-map.ts';
import { readNozzleLayout } from '../src/nozzle-layout.ts';
import { buildCarBody } from '../src/render/car-mesh.ts';
import { createFlames, updateFlames } from '../src/render/flames.ts';
import { BODY_BEVEL_M, bodyTopM, flankBottomM } from '../src/render/car-shape.ts';
import { SIDE_PORT_RADIUS_M, portPositionM, portSurface } from '../src/render/nozzle-ports.ts';
import { Game, controllersToml, modesToml, vehicleToml } from './load-wasm.mjs';

function nozzleLayout() {
  return new Game(vehicleToml, modesToml, controllersToml, 0n).nozzle_layout();
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
  // Each nozzle is a port plus its flame, both on the car body.
  assert.equal(carBody.children.length, 20);
  assert.equal(flames[0].parent, carBody);
});

test('every port sits on the body surface, a short way from its nozzle', () => {
  const carBody = buildCarBody();
  const bodyBox = new Box3().setFromObject(carBody.children[0]);
  const deepInside = bodyBox.clone().expandByScalar(-0.03);
  const nozzles = readNozzleLayout(nozzleLayout());

  nozzles.forEach((place, index) => {
    const port = new Vector3().fromArray(portPositionM(place));
    const nozzle = new Vector3().fromArray(place.positionM);

    assert.ok(bodyBox.distanceToPoint(port) < 0.03, `port ${index} touches the body`);
    assert.ok(!deepInside.containsPoint(port), `port ${index} is on the surface`);
    assert.ok(port.distanceTo(nozzle) < 0.45, `port ${index} is near its nozzle`);
  });
});

test('every side port sits on the flat flank, clear of the rounded top edge and arch lip', () => {
  const nozzles = readNozzleLayout(nozzleLayout());
  const sidePorts = nozzles.filter((place) => portSurface(place) === 'side');

  assert.equal(sidePorts.length, 4);
  for (const place of sidePorts) {
    const [forwardM, upM] = portPositionM(place);
    // The bevel is measured square to the top edge, so it reaches further down where the
    // edge slopes.
    const slope = (bodyTopM(forwardM - 0.01) - bodyTopM(forwardM + 0.01)) / 0.02;
    const flatTopM = bodyTopM(forwardM) - BODY_BEVEL_M * Math.hypot(1, slope);
    const flatBottomM = flankBottomM(forwardM) + BODY_BEVEL_M;
    assert.ok(
      upM + SIDE_PORT_RADIUS_M <= flatTopM,
      `port at x ${forwardM}: top ${upM + SIDE_PORT_RADIUS_M} m pokes above the flat at ${flatTopM} m`,
    );
    assert.ok(
      upM - SIDE_PORT_RADIUS_M >= flatBottomM,
      `port at x ${forwardM}: bottom ${upM - SIDE_PORT_RADIUS_M} m dips below the flat at ${flatBottomM} m`,
    );
  }
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

/** Stand-ins for the gauge's page elements: only what the module touches. */
function fakeGauge() {
  const classes = new Set();
  const box = {
    classList: { toggle: (name, on) => (on ? classes.add(name) : classes.delete(name)) },
  };
  return { gauge: createAirGauge(box, { style: { width: '' } }, { textContent: '' }), classes };
}

test('the air gauge flashes the moment air drops below 15%, not a percent later', () => {
  const { gauge, classes } = fakeGauge();

  updateAirGauge(gauge, 0.152, false);
  assert.equal(classes.has('low'), false);
  updateAirGauge(gauge, 0.149, false);

  assert.equal(gauge.label.textContent, '15%');
  assert.equal(classes.has('low'), true);
  assert.equal(gauge.fill.style.width, '14.9%');
});

test('unlimited air shows ∞, a full bar and never flashes', () => {
  const { gauge, classes } = fakeGauge();

  updateAirGauge(gauge, 0, true);

  assert.equal(gauge.label.textContent, '∞');
  assert.equal(gauge.fill.style.width, '100.0%');
  assert.equal(classes.has('low'), false);
});
