// Entry point for the web game. It wires the pieces together:
// wasm physics (Game) ← keyboard input, then the scene draws blended poses each frame.
// Each piece lives in its own folder: input/, render/, hud/; the loop is in game-loop.ts.
import vehicleToml from '../../configs/vehicle.toml?raw';

import { startGameLoop } from './game-loop';
import { findHud, updateHud } from './hud/hud';
import { readInputFrame, startKeyboard } from './input/keyboard';
import { followCar, snapCamera } from './render/chase-camera';
import { blendPose, newBlendedPose } from './render/pose-blend';
import { createGameScene } from './render/scene';
import initWasm, { Game, HudField } from './wasm/nl_wasm.js';

const WHEEL_COUNT = 4;

async function start(): Promise<void> {
  await initWasm();
  const game = new Game(vehicleToml);
  const tickS = Game.physics_dt_s();

  const container = document.getElementById('game');
  if (!container) {
    throw new Error('The page has no #game element');
  }
  const view = createGameScene(container);
  const hud = findHud(document);
  startKeyboard(window);

  // The last two physics states, so each frame can blend between them.
  let previousChassis = game.chassis_pose();
  let latestChassis = previousChassis;
  let previousWheels = game.wheel_poses();
  let latestWheels = previousWheels;

  const carPose = newBlendedPose();
  const wheelPose = newBlendedPose();
  blendPose(previousChassis, latestChassis, 0, 1, carPose);
  snapCamera(view.camera, carPose);

  let lastRenderMs = performance.now();

  startGameLoop(tickS, {
    tick: () => {
      previousChassis = latestChassis;
      previousWheels = latestWheels;
      game.tick(readInputFrame());
      latestChassis = game.chassis_pose();
      latestWheels = game.wheel_poses();
    },
    render: (blend) => {
      blendPose(previousChassis, latestChassis, 0, blend, carPose);
      view.carBody.position.copy(carPose.position);
      view.carBody.quaternion.copy(carPose.rotation);

      for (let wheel = 0; wheel < WHEEL_COUNT; wheel += 1) {
        blendPose(previousWheels, latestWheels, wheel, blend, wheelPose);
        view.wheels[wheel]?.position.copy(wheelPose.position);
        view.wheels[wheel]?.quaternion.copy(wheelPose.rotation);
      }

      // Keep the sun's shadow box centred on the car.
      view.sun.position.copy(carPose.position).add({ x: 30, y: 60, z: 20 });
      view.sun.target.position.copy(carPose.position);

      const nowMs = performance.now();
      followCar(view.camera, carPose, (nowMs - lastRenderMs) / 1000);
      lastRenderMs = nowMs;

      updateHud(hud, game.hud()[HudField.SpeedMS] ?? 0);
      view.renderer.render(view.scene, view.camera);
    },
  });

  // A small read-only hook so automated tests can see where the car is.
  Object.assign(window, { ninelivesDebug: { chassisPose: () => Array.from(game.chassis_pose()) } });
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
