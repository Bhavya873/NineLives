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

test('about 20 debris pieces fly up and out, then fall', () => {
  const explosion = createExplosion(new Scene(), false);
  startExplosion(explosion, new Vector3(0, 1, 0));

  play(explosion, 0.2);
  for (const piece of explosion.debris) {
    assert.ok(piece.mesh.position.y > 0, `piece at y ${piece.mesh.position.y}`);
  }
  const risingAt = explosion.debris.map((piece) => piece.velocityMS.y);
  play(explosion, 1.2);

  assert.equal(explosion.debris.length, 20);
  explosion.debris.forEach((piece, index) => {
    assert.ok(piece.velocityMS.y < (risingAt[index] ?? 0), 'gravity slows every piece');
  });
});

test('the flash light stays in the scene and goes dark after the effect', () => {
  const scene = new Scene();
  const explosion = createExplosion(scene, false);
  startExplosion(explosion, new Vector3());

  play(explosion, 0.05);
  assert.ok(explosion.flash.intensity > 0);
  play(explosion, 2);

  assert.equal(explosion.flash.intensity, 0);
  assert.equal(explosion.flash.parent, scene);
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
