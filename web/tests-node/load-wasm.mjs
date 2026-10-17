// Loads the wasm build from web/src/wasm in Node, without a browser or a server.
// Shared by the Node tests and scripts/measure-tick.mjs. Run `npm run wasm` (or
// `npm run wasm:release`) first.
import { readFileSync } from 'node:fs';

import * as wasm from '../src/wasm/nl_wasm.js';

wasm.initSync({ module: readFileSync(new URL('../src/wasm/nl_wasm_bg.wasm', import.meta.url)) });

/** The wasm module's exports (Game, HudField, ...), ready to use. */
export const { ControllerCode, CrashCode, EventKind, Game, HopBlocked, HudField, ModeCode } = wasm;

/** The text of the repo's configs/modes.toml. */
export const modesToml = readFileSync(new URL('../../configs/modes.toml', import.meta.url), 'utf8');

/** The text of the repo's configs/vehicle.toml. */
export const vehicleToml = readFileSync(
  new URL('../../configs/vehicle.toml', import.meta.url),
  'utf8',
);
