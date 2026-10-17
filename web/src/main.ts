// Entry point for the web game.
// The next PR replaces this placeholder with the renderer, input and game loop. For
// now it loads the wasm module and shows the build name, to prove the build works.
import initWasm, { Game } from './wasm/nl_wasm.js';

async function start(): Promise<void> {
  await initWasm();

  const buildInfoElement = document.getElementById('build-info');
  if (buildInfoElement) {
    buildInfoElement.textContent = `${Game.build_hash()} · physics ${1 / Game.physics_dt_s()} Hz`;
  }
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
