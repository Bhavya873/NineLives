// Measures how long one physics tick takes in wasm (SPEC §11.7 budget: 1.5 ms).
//
// Usage: npm run wasm:release && node scripts/measure-tick.mjs
// Run it on a release build; debug builds are many times slower.
import { performance } from 'node:perf_hooks';

import { Game, modesToml, vehicleToml } from '../tests-node/load-wasm.mjs';

const WARM_UP_TICKS = 600;
const MEASURED_TICKS = 6000;
const TICK_BUDGET_MS = 1.5;

const game = new Game(vehicleToml, modesToml);
const throttleAndTurn = new Uint8Array([100, 40, 0, 0]);

for (let tick = 0; tick < WARM_UP_TICKS; tick += 1) {
  game.tick(throttleAndTurn);
}

const startMs = performance.now();
for (let tick = 0; tick < MEASURED_TICKS; tick += 1) {
  game.tick(throttleAndTurn);
  // The browser reads the poses every frame too, so include that cost.
  game.chassis_pose();
  game.wheel_poses();
}
const perTickMs = (performance.now() - startMs) / MEASURED_TICKS;

console.log(`wasm tick: ${perTickMs.toFixed(4)} ms (budget ${TICK_BUDGET_MS} ms)`);
