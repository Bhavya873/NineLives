// The HUD pieces the frame loop updates every frame: readouts, toasts, nozzle map and flames.
// They are found or built once at start-up so the render loop only updates them.
import { requireElement } from '../shared/dom.ts';
import { createAirGauge } from '../ui/air-gauge.ts';
import { findHud } from '../ui/hud.ts';
import { newZoneHolds } from '../ui/hud-zones.ts';
import { createNozzleMap } from '../ui/nozzle-map.ts';
import { createToasts } from '../ui/toasts.ts';
import { createFlames } from '../render/flames.ts';
import { readNozzleLayout } from '../wasm-bridge/nozzle-layout.ts';
import type { Game } from '../wasm/nl_wasm.js';
import type { World } from './world.ts';

/** The HUD elements and effects that follow the sim each frame. */
export interface HudParts {
  hud: ReturnType<typeof findHud>;
  toasts: ReturnType<typeof createToasts>;
  flames: ReturnType<typeof createFlames>;
  nozzleDots: ReturnType<typeof createNozzleMap>;
  airGauge: ReturnType<typeof createAirGauge>;
  zoneTiles: { gTile: HTMLElement; nozzleTile: HTMLElement };
  zoneHolds: ReturnType<typeof newZoneHolds>;
  nozzleWho: HTMLElement;
  savesCounter: { count: HTMLElement; label: HTMLElement };
}

/** Finds the HUD elements by id and builds the nozzle map and the flames on the car. */
export function createHudParts(game: Game, world: World): HudParts {
  const nozzles = readNozzleLayout(game.nozzle_layout());
  return {
    hud: findHud(),
    toasts: createToasts(requireElement('hud-toast')),
    flames: createFlames(world.view.carBody, nozzles),
    nozzleDots: createNozzleMap(requireElement('hud-nozzles'), nozzles),
    airGauge: createAirGauge(
      requireElement('hud-air'),
      requireElement('hud-air-fill'),
      requireElement('hud-air-label'),
    ),
    zoneTiles: {
      gTile: requireElement('hud-g-tile'),
      nozzleTile: requireElement('hud-nozzle-tile'),
    },
    zoneHolds: newZoneHolds(),
    nozzleWho: requireElement('hud-nozzle-who'),
    savesCounter: {
      count: requireElement('hud-saves-count'),
      label: requireElement('hud-saves-label'),
    },
  };
}
