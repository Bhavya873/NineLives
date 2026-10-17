// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
// Each piece lives in its own folder: input/, render/, hud/; the loop is in game-loop.ts.
import modesToml from '../../configs/modes.toml?raw';
import vehicleToml from '../../configs/vehicle.toml?raw';

import { requireElement } from './dom';
import { startGameLoop } from './game-loop';
import { findHud, updateHud } from './hud/hud';
import { onKeyPress, readInputFrame, startKeyboard } from './input/keyboard';
import { followCar, snapCamera } from './render/chase-camera';
import { FLOATS_PER_POSE, newBlendedPose } from './render/pose-blend';
import { createGameScene, type PhysicsFrames, placeCar } from './render/scene';
import initWasm, { Game, HudField } from './wasm/nl_wasm.js';

async function start(): Promise<void> {
  await initWasm();
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the renderer disagree on the pose layout');
  }
  const game = new Game(vehicleToml, modesToml);
  onKeyPress('KeyM', () => {
    game.set_mode(game.mode_name() === 'arcade' ? 'realistic' : 'arcade');
  });

  const frames: PhysicsFrames = {
    previousChassis: game.chassis_pose(),
    latestChassis: game.chassis_pose(),
    previousWheels: game.wheel_poses(),
    latestWheels: game.wheel_poses(),
  };
  const wheelCount = frames.latestWheels.length / FLOATS_PER_POSE;
  const view = createGameScene(requireElement('game'), wheelCount);
  const hud = findHud();
  startKeyboard(window);

  const carPose = newBlendedPose();
  placeCar(view, frames, 1, carPose);
  snapCamera(view.camera, carPose);
  let lastRenderMs = performance.now();

  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      frames.previousChassis = frames.latestChassis;
      frames.previousWheels = frames.latestWheels;
      game.tick(readInputFrame());
      frames.latestChassis = game.chassis_pose();
      frames.latestWheels = game.wheel_poses();
    },
    render: (blend) => {
      const nowMs = performance.now();
      const hudValues = game.hud();
      const airborne = hudValues[HudField.WheelsTouching] === 0;

      placeCar(view, frames, blend, carPose);
      followCar(view.camera, carPose, (nowMs - lastRenderMs) / 1000, airborne);
      updateHud(hud, {
        speedMS: hudValues[HudField.SpeedMS] ?? 0,
        modeName: game.mode_name(),
        airFraction: hudValues[HudField.AirFraction] ?? 1,
        unlimitedAir: hudValues[HudField.UnlimitedAir] === 1,
        hopBlocked: hudValues[HudField.HopBlocked] ?? 0,
      });
      view.renderer.render(view.scene, view.camera);
      lastRenderMs = nowMs;
    },
  });

  // A small read-only hook so automated tests can see where the car is.
  Object.assign(window, { ninelivesDebug: { chassisPose: () => Array.from(game.chassis_pose()) } });
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
