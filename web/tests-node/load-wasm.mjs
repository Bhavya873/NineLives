// Loads the wasm build from web/src/wasm in Node, without a browser or a server.
// Run `npm run wasm` (or `npm run wasm:release`) first.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as wasm from '../src/wasm/nl_wasm.js';

const wasmDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'wasm');

/** Returns the wasm module's exports, ready to use. */
export function loadWasm() {
  wasm.initSync({ module: readFileSync(join(wasmDir, 'nl_wasm_bg.wasm')) });
  return wasm;
}
