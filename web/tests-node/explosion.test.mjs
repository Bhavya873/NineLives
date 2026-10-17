// Unit tests for the crash explosion (SPEC §7.9, §11.4): it plays for about the respawn
// delay, throws debris up and out, and never shakes the camera with reduced motion.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Scene, Vector3 } from 'three';

import {
  cameraShake,
  createExplosion,
  isExplosionPlaying,
  startExplosion,
  updateExplosion,
} from '../src/render/explosion.ts';

/** Advances the explosion in 60 Hz frames for `seconds`. */
function play(explosion, seconds) {
  for (let frame = 0; frame < Math.round(seconds * 60); frame += 1) {
    updateExplosion(explosion, 1 / 60);
  }
}

test('the explosion plays for about 1.5 s and then hides', () => {
  const explosion = createExplosion(new Scene(), false);

  startExplosion(explosion, new Vector3(5, 1, 2));
  play(explosion, 1.0);
  assert.equal(isExplosionPlaying(explosion), true);
  assert.equal(explosion.group.visible, true);
  play(explosion, 0.6);

  assert.equal(isExplosionPlaying(explosion), false);
  assert.equal(explosion.group.visible, false);
});

test('about 20 debris pieces fly up and come to rest on the ground', () => {
  const explosion = createExplosion(new Scene(), false);
  startExplosion(explosion, new Vector3(0, 1, 0));

  play(explosion, 0.2);
  for (const piece of explosion.debris) {
    assert.ok(piece.mesh.position.y > 0, `piece at y ${piece.mesh.position.y}`);
  }
  play(explosion, 1.2);

  assert.equal(explosion.debris.length, 20);
  for (const piece of explosion.debris) {
    // Group space: the ground is 1 m below the crash point.
    assert.ok(piece.mesh.position.y >= -1, `piece below the ground at ${piece.mesh.position.y}`);
  }
});

test('reduced motion means no camera shake and fewer pieces', () => {
  const explosion = createExplosion(new Scene(), true);
  startExplosion(explosion, new Vector3());
  play(explosion, 0.05);

  assert.equal(cameraShake(explosion, new Vector3()).length(), 0);
  assert.ok(explosion.debris.length < 20);
});

test('the camera shakes right after a crash and settles within half a second', () => {
  const explosion = createExplosion(new Scene(), false);
  startExplosion(explosion, new Vector3());

  play(explosion, 0.05);
  const early = cameraShake(explosion, new Vector3()).length();
  play(explosion, 0.5);
  const late = cameraShake(explosion, new Vector3()).length();

  assert.ok(early > 0);
  assert.equal(late, 0);
});
