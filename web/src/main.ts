// Entry point for the web game.
// M2 replaces this placeholder with the renderer, input and game loop. For now
// it loads the wasm module and shows its build info, to prove the build works.
import initWasm, { build_info } from './wasm/nl_wasm.js';

async function start(): Promise<void> {
  await initWasm();

  const buildInfoElement = document.getElementById('build-info');
  if (buildInfoElement) {
    buildInfoElement.textContent = build_info();
  }
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
