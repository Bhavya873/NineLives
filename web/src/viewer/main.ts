// The training viewer (SPEC §11.6): every stage at once. Each stage (the flat arena, Stage B,
// the obstacle arena, Stage D, Stage F) has its own panel; in each, every exported checkpoint
// of the newest training run flies eight drops, one labelled row of translucent cars per
// checkpoint, all on the same drops, so you can see the policy improve from row to row. Each
// stage is stepped by its own Web Worker (stage-worker.ts), so this page only draws: one
// renderer and one scene, with every stage drawn into its own rectangle of the canvas, and one
// pool of instanced meshes per stage. The stage tabs, the number keys, a click on a panel's
// title or `?arena=…` focus one panel; Esc or the All tab tiles them again.
import type { Scene } from 'three';

import { requireElement, setText } from '../shared/dom.ts';
import {
  createPerfOverlay,
  recordFrame,
  recordTick,
  togglePerfOverlay,
} from '../ui/perf-overlay.ts';
import { createArena, setArenaPixelRatio } from '../render/scene.ts';
import { newQuality, recordQualityFrame } from './adaptive-quality.ts';
import {
  ARENAS,
  type ArenaKind,
  arenaFromUrl,
  arenaLabel,
  followsCars,
  searchWithArena,
} from './arena-choice.ts';
import {
  type RunEntry,
  loadCheckpoint,
  loadCurve,
  loadNewestRun,
  loadShowcase,
} from './checkpoints.ts';
import { CURVE_LABEL, type CurveFile, curveSvg } from './curve.ts';
import { type Rect, layoutPanels } from './panel-layout.ts';
import { STAGE_TABS, type StageTab, arenaTab, showcaseSeed, tabArena } from './showcase.ts';
import {
  type CameraMode,
  type StagePanel,
  aimStageCamera,
  createStagePanel,
  setStageAspect,
  setStageRunning,
  stageTotals,
  updateStagePanel,
} from './stage-panel.ts';

/** Cars per checkpoint: 8 checkpoints × 8 cars is SPEC §11.6's crowd of about 64 per stage. */
const CARS_PER_CHECKPOINT = 8;
/** How long the final picture stays before the next round of drops, in ms. */
const PAUSE_BETWEEN_MS = 2500;
/** The crowd is many see-through cars, so the viewer draws at most this many pixels per CSS pixel. */
const VIEWER_MAX_PIXEL_RATIO = 1.5;
/** The longest frame the cars are moved for, in s, so a hidden tab does not jump the picture. */
const MAX_FRAME_S = 0.1;

/** The font of the car labels (crowd-view.ts), loaded before they are drawn. */
const CAR_LABEL_FONT = '700 40px "Barlow Condensed"';

/** Where the viewer is: the run, its stages' panels, the focus and the camera. */
interface ViewerState {
  run: RunEntry;
  panels: StagePanel[];
  /** The stage that fills the screen, or null to show them all. */
  focus: ArenaKind | null;
  /** The tab (stage) the curve shows. */
  tab: StageTab;
  /** What the cameras of the course panels follow. */
  camera: CameraMode;
}

/** A stage panel's title on the page, and the rectangle it sits in. */
interface PanelLabel {
  element: HTMLButtonElement;
  heading: HTMLElement;
  detail: HTMLElement;
}

/** What the viewer loads from the network before it can draw anything. */
interface ViewerData {
  run: RunEntry;
  curve: CurveFile | null;
  showcase: Awaited<ReturnType<typeof loadShowcase>> | null;
  policies: string[];
}

/** The frame timing the loop and the visibility handler share. */
interface FrameTiming {
  lastFrameStartMs: number;
  /** Total main-thread ms spent in frames since the last reset, and how many frames. */
  mainMsTotal: number;
  mainFrames: number;
}

async function start(): Promise<void> {
  const data = await loadViewerData();
  const arena = createViewerArena();
  const panels = createPanels(arena.scene, data);
  const focus = arenaFromUrl(window.location.search);
  const state: ViewerState = {
    run: data.run,
    panels,
    focus,
    tab: arenaTab(focus ?? 'flat'),
    camera:
      new URLSearchParams(window.location.search).get('camera') === 'leader' ? 'leader' : 'crowd',
  };
  const board = createBoard(requireElement('viewer-board'), data.run);
  const cameraButton = requireElement('camera-toggle');
  cameraButton.setAttribute('aria-keyshortcuts', 'L');
  const { setFocus, applyRunning } = wireFocus(state, data.curve);
  const panelLabels = createPanelLabels(state, setFocus);
  setFocus(focus);
  const toggleCamera = () => {
    state.camera = state.camera === 'crowd' ? 'leader' : 'crowd';
  };
  cameraButton.addEventListener('click', toggleCamera);
  const perf = createPerfOverlay(requireElement('hud-perf'), performance.now());
  const perfSteps = requireElement('hud-perf-steps');
  readReservedSideWidth();
  window.addEventListener('resize', readReservedSideWidth);
  installKeys(toggleCamera, setFocus, perf);
  const timing = startFrameLoop(state, arena, panelLabels, board, perf, perfSteps, cameraButton);
  document.addEventListener('visibilitychange', () => {
    applyRunning();
    if (!document.hidden) {
      timing.lastFrameStartMs = performance.now();
    }
  });
  exposeTestHooks(state, arena, timing, data);
}

/** Loads the newest run, its curve, its showcase seeds, its checkpoints and the label font. */
async function loadViewerData(): Promise<ViewerData> {
  const run = await loadNewestRun();
  const curve = await loadCurve(run).catch((error: unknown) => {
    console.error(error);
    return null;
  });
  const showcase = await loadShowcase(run).catch(() => null);
  const policies = await Promise.all(run.checkpoints.map((entry) => loadCheckpoint(run, entry)));
  // The car labels are drawn once onto canvases, so the font must be there before they are.
  await document.fonts.load(CAR_LABEL_FONT).catch(() => []);
  return { run, curve, showcase, policies };
}

/** The renderer and scene of the page's canvas, set up for a crowd of see-through cars. */
function createViewerArena(): ReturnType<typeof createArena> {
  const arena = createArena(requireElement('viewer'), VIEWER_MAX_PIXEL_RATIO);
  arena.renderer.setClearColor('#16202b');
  // The sun's shadow box covers only 30 m around the origin and the crowd casts no shadows, so
  // the viewer skips the shadow pass (it would run once per panel).
  arena.renderer.shadowMap.enabled = false;
  return arena;
}

/** One stage panel (and its worker) per arena. */
function createPanels(scene: Scene, data: ViewerData): StagePanel[] {
  const labels = rowLabels(data.run);
  return ARENAS.map((kind, index) =>
    createStagePanel(scene, {
      arena: kind,
      layer: index + 1,
      policies: data.policies,
      rowLabels: labels,
      carsPerCheckpoint: CARS_PER_CHECKPOINT,
      firstSeed: showcaseSeed(data.showcase, kind, 0, CARS_PER_CHECKPOINT),
      pauseBetweenMs: PAUSE_BETWEEN_MS,
    }),
  );
}

/** Builds the stage tabs and curve, and returns the focus setter and the run/stop switch. */
function wireFocus(
  state: ViewerState,
  curve: CurveFile | null,
): { setFocus: (next: ArenaKind | null) => void; applyRunning: () => void } {
  // A stage runs only while it can be seen: it is in the focus and the tab is showing.
  const applyRunning = () => {
    for (const panel of state.panels) {
      const shown = state.focus === null || state.focus === panel.arena;
      setStageRunning(panel, shown && !document.hidden);
    }
  };
  const setFocus = (next: ArenaKind | null) => {
    state.focus = next;
    if (next !== null) {
      state.tab = arenaTab(next);
    }
    applyRunning();
    window.history.replaceState(
      null,
      '',
      `${window.location.pathname}${searchWithArena(window.location.search, next)}`,
    );
    curvePanel.select(state.tab);
    curvePanel.markFocus(next);
  };
  const curvePanel = createCurvePanel(
    curve,
    state,
    (tab) => {
      const kind = tabArena(tab);
      if (kind) {
        setFocus(kind);
      }
    },
    () => setFocus(null),
  );
  return { setFocus, applyRunning };
}

/** One clickable title per panel, added to the labels layer. */
function createPanelLabels(
  state: ViewerState,
  setFocus: (next: ArenaKind | null) => void,
): Map<ArenaKind, PanelLabel> {
  const labelsLayer = requireElement('viewer-panels');
  const panelLabels = new Map<ArenaKind, PanelLabel>();
  for (const panel of state.panels) {
    const element = document.createElement('button');
    element.type = 'button';
    element.className = 'tile panel-title';
    element.dataset.arena = panel.arena;
    const heading = document.createElement('span');
    heading.className = 'panel-heading';
    const detail = document.createElement('span');
    detail.className = 'panel-detail';
    element.append(heading, detail);
    element.addEventListener('click', () =>
      setFocus(state.focus === panel.arena ? null : panel.arena),
    );
    labelsLayer.appendChild(element);
    panelLabels.set(panel.arena, { element, heading, detail });
  }
  return panelLabels;
}

/** The keyboard: L camera, F3 perf overlay, Esc or 0 all stages, 1-9 one stage. */
function installKeys(
  toggleCamera: () => void,
  setFocus: (next: ArenaKind | null) => void,
  perf: ReturnType<typeof createPerfOverlay>,
): void {
  window.addEventListener('keydown', (event) => {
    if (event.code === 'KeyL') {
      toggleCamera();
    } else if (event.code === 'F3') {
      event.preventDefault();
      togglePerfOverlay(perf);
    } else if (event.code === 'Escape' || event.code === 'Digit0') {
      setFocus(null);
    } else if (/^Digit[1-9]$/.test(event.code)) {
      const kind = ARENAS[Number(event.code.slice(5)) - 1];
      if (kind) {
        setFocus(kind);
      }
    }
  });
}

/** Starts the frame loop and returns its timing (the visibility handler resets it). */
function startFrameLoop(
  state: ViewerState,
  arena: ReturnType<typeof createArena>,
  panelLabels: Map<ArenaKind, PanelLabel>,
  board: Board,
  perf: ReturnType<typeof createPerfOverlay>,
  perfSteps: HTMLElement,
  cameraButton: HTMLElement,
): FrameTiming {
  const status = requireElement('viewer-status');
  // On a slow GPU the resolution steps down by itself (adaptive-quality.ts).
  const quality = newQuality(arena.renderer.getPixelRatio());
  const timing: FrameTiming = {
    lastFrameStartMs: performance.now(),
    mainMsTotal: 0,
    mainFrames: 0,
  };
  const frame = (frameStartMs: number) => {
    // Ask for the next frame first, so one error cannot stop the viewer for good.
    requestAnimationFrame(frame);
    try {
      const realDtS = Math.min((frameStartMs - timing.lastFrameStartMs) / 1000, MAX_FRAME_S);
      drawFrame(state, arena, panelLabels, realDtS, perf, perfSteps);
      setText(cameraButton, state.camera === 'leader' ? 'camera: leader' : 'camera: crowd');
      setText(status, statusText(state));
      updateBoard(board, state.panels);
      const frameEndMs = performance.now();
      timing.mainMsTotal += frameEndMs - frameStartMs;
      timing.mainFrames += 1;
      recordFrame(perf, frameEndMs - frameStartMs, frameEndMs);
      const lowerRatio = recordQualityFrame(quality, frameStartMs - timing.lastFrameStartMs);
      if (lowerRatio !== null) {
        setArenaPixelRatio(arena, lowerRatio);
      }
      timing.lastFrameStartMs = frameStartMs;
    } catch (error) {
      console.error('Nine Lives viewer: error in the frame loop:', error);
    }
  };
  requestAnimationFrame(frame);
  return timing;
}

/** `window.ninelivesViewer`: the read-only handles the Playwright tests use. */
function exposeTestHooks(
  state: ViewerState,
  arena: ReturnType<typeof createArena>,
  timing: FrameTiming,
  data: ViewerData,
): void {
  const { panels } = state;
  Object.assign(window, {
    ninelivesViewer: {
      checkpointCount: () => data.run.checkpoints.length,
      landed: () => panels.reduce((sum, panel) => sum + stageTotals(panel).landed, 0),
      focus: () => state.focus,
      /** The focused arena, or 'all'. */
      arena: () => state.focus ?? 'all',
      courseShapeCount: (kind?: ArenaKind) =>
        panels
          .filter((panel) => kind === undefined || panel.arena === kind)
          .reduce((sum, panel) => sum + panel.shapeCount, 0),
      stages: () =>
        panels.map((panel) => ({
          arena: panel.arena,
          round: panel.round,
          rows: panel.rows.length,
          landed: stageTotals(panel).landed,
          crashed: stageTotals(panel).crashed,
          shapeCount: panel.shapeCount,
          running: panel.running,
          stepping: panel.stepping,
          stepMs: panel.stepMs,
          visible: state.focus === null || state.focus === panel.arena,
        })),
      tab: () => state.tab,
      leaderCamera: () => state.camera === 'leader',
      pixelRatio: () => arena.renderer.getPixelRatio(),
      mainMsPerFrame: () => (timing.mainFrames > 0 ? timing.mainMsTotal / timing.mainFrames : 0),
      resetMainTiming: () => {
        timing.mainMsTotal = 0;
        timing.mainFrames = 0;
      },
      firstSeed: (kind: ArenaKind) => showcaseSeed(data.showcase, kind, 0, CARS_PER_CHECKPOINT),
    },
  });
}

/** Width the side column takes from the panels, in CSS pixels. viewer.css owns this number
 * (--viewer-side-reserved, 0 where the column floats over the panels); it is re-read on resize. */
let reservedSideWidthPx = 0;

function readReservedSideWidth(): void {
  const value = getComputedStyle(document.documentElement).getPropertyValue(
    '--viewer-side-reserved',
  );
  const px = Number.parseFloat(value);
  reservedSideWidthPx = Number.isFinite(px) ? px : 0;
}

/** The rectangle the panels share: the window, less the column on the right. */
function panelArea(): Rect {
  return {
    x: 0,
    y: 0,
    w: window.innerWidth - reservedSideWidthPx,
    h: window.innerHeight,
  };
}

/** Draws every visible stage panel into its rectangle of the canvas. */
function drawFrame(
  state: ViewerState,
  arena: ReturnType<typeof createArena>,
  panelLabels: Map<ArenaKind, PanelLabel>,
  realDtS: number,
  perf: ReturnType<typeof createPerfOverlay>,
  perfSteps: HTMLElement,
): void {
  const { renderer } = arena;
  const rects = layoutPanels(panelArea(), state.focus);
  // The gaps and the column on the right show the clear colour.
  renderer.setScissorTest(false);
  renderer.setViewport(0, 0, window.innerWidth, window.innerHeight);
  renderer.clear();
  renderer.setScissorTest(true);
  let stepMsTotal = 0;
  for (const panel of state.panels) {
    const rect = rects.get(panel.arena);
    const label = panelLabels.get(panel.arena);
    if (label) {
      placeLabel(label, rect, panel);
    }
    if (!rect) {
      continue;
    }
    stepMsTotal += panel.stepMs;
    drawPanel(state, arena, panel, rect, realDtS);
  }
  renderer.setScissorTest(false);
  // The workers' stepping cost is what the F3 "tick" figure shows: all stages together.
  recordTick(perf, stepMsTotal);
  showStepTimes(state.panels, perf, perfSteps);
}

/** Moves one panel's cars and camera, then renders it into its rectangle. */
function drawPanel(
  state: ViewerState,
  arena: ReturnType<typeof createArena>,
  panel: StagePanel,
  rect: Rect,
  realDtS: number,
): void {
  const { renderer, scene } = arena;
  const focused = state.focus === panel.arena;
  updateStagePanel(panel, realDtS, labelMode(panel.arena, focused));
  setStageAspect(panel, rect.w, rect.h);
  aimStageCamera(panel, state.camera);
  // A course brings its own floor; the endless floor is only for the drop arenas.
  arena.ground.visible = panel.shapeCount === 0;
  // three.js counts the viewport's y from the bottom of the canvas.
  const bottomY = window.innerHeight - (rect.y + rect.h);
  renderer.setViewport(rect.x, bottomY, rect.w, rect.h);
  renderer.setScissor(rect.x, bottomY, rect.w, rect.h);
  renderer.render(scene, panel.camera);
}

/** The F3 line with each worker's ms per tick, shown only while the overlay is open. */
function showStepTimes(
  panels: StagePanel[],
  perf: ReturnType<typeof createPerfOverlay>,
  perfSteps: HTMLElement,
): void {
  perfSteps.hidden = perf.element.hidden;
  if (!perfSteps.hidden) {
    perfSteps.textContent = `worker ms per tick: ${panels
      .map((panel) => `${panel.arena} ${panel.stepMs.toFixed(2)}`)
      .join(' · ')}`;
  }
}

/** Which labels a panel shows: all of them when it has the screen to itself; otherwise one
 * per checkpoint row on the drop grids and none on the courses (where the cars move). */
function labelMode(arena: ArenaKind, focused: boolean): 'cars' | 'row' | 'off' {
  if (focused) {
    return 'cars';
  }
  return followsCars(arena) || arena === 'obstacles' ? 'off' : 'row';
}

/** Puts a panel's title over its rectangle's top-left corner, or hides it. */
function placeLabel(label: PanelLabel, rect: Rect | undefined, panel: StagePanel): void {
  label.element.hidden = !rect;
  if (!rect) {
    return;
  }
  label.element.style.left = `${rect.x + 8}px`;
  label.element.style.top = `${rect.y + 8}px`;
  // A narrow panel (a phone held sideways) shortens its title with an ellipsis instead of
  // running over the next panel.
  label.element.style.maxWidth = `${Math.max(0, rect.w - 16)}px`;
  const totals = stageTotals(panel);
  const cars = panel.rows.length * CARS_PER_CHECKPOINT;
  setText(label.heading, arenaLabel(panel.arena));
  setText(label.detail, `${totals.landed}/${cars} landed`);
}

/** The stage tabs, the All tab and the curve under them: the chart of the selected stage,
 * titled as the plan requires, with a note on tabs whose stage the crowd cannot fly. */
function createCurvePanel(
  curve: CurveFile | null,
  state: ViewerState,
  onTab: (tab: StageTab) => void,
  onAll: () => void,
): { select: (tab: StageTab) => void; markFocus: (focus: ArenaKind | null) => void } {
  const tabs = requireElement('stage-tabs');
  const chart = requireElement('curve-chart');
  setText(requireElement('curve-title'), CURVE_LABEL);
  const buttons = new Map<StageTab, HTMLButtonElement>();
  const all = document.createElement('button');
  all.type = 'button';
  all.textContent = 'All';
  all.setAttribute('aria-keyshortcuts', 'Escape');
  all.addEventListener('click', onAll);
  tabs.appendChild(all);
  const select = (tab: StageTab) => {
    state.tab = tab;
    for (const [name, button] of buttons) {
      button.setAttribute('aria-pressed', String(name === tab && state.focus !== null));
    }
    if (!curve) {
      chart.textContent = 'no curve data';
      return;
    }
    const note = tabArena(tab) ? '' : '<p>curve only</p>';
    const facts = `<p>${curve.episodes} drops per point</p>`;
    chart.innerHTML = curveSvg(curve, tab) + note + facts;
  };
  const markFocus = (focus: ArenaKind | null) => {
    all.setAttribute('aria-pressed', String(focus === null));
    for (const [name, button] of buttons) {
      button.setAttribute('aria-pressed', String(focus !== null && name === state.tab));
    }
  };
  for (const tab of STAGE_TABS) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = tab;
    // the number key that focuses this stage (the curve-only tabs have none)
    const kind = tabArena(tab);
    if (kind) {
      const key = String(ARENAS.indexOf(kind) + 1);
      button.dataset.key = key;
      button.setAttribute('aria-keyshortcuts', key);
    }
    button.addEventListener('click', () => {
      select(tab);
      onTab(tab);
    });
    tabs.appendChild(button);
    buttons.set(tab, button);
  }
  select(state.tab);
  markFocus(state.focus);
  return { select, markFocus };
}

/** A checkpoint's training steps in millions, e.g. "16M". */
function stepsText(steps: number): string {
  return `${Number((steps / 1e6).toFixed(1))}M`;
}

/** Each checkpoint's label over its cars, e.g. "#3 (16M)". */
function rowLabels(run: RunEntry): string[] {
  return run.checkpoints.map((entry, index) => `#${index + 1} (${stepsText(entry.steps)})`);
}

/** The scoreboard: one row per checkpoint, one cell per stage with that stage's landed and
 * crashed counts. */
interface Board {
  /** `cells[row][stage]` */
  cells: HTMLElement[][];
}

/** Fills `table` with one row per checkpoint and one column per stage. */
function createBoard(table: HTMLElement, run: RunEntry): Board {
  const header = document.createElement('tr');
  for (const title of ['Checkpoint', 'Steps', ...ARENAS.map(shortLabel)]) {
    const cell = document.createElement('th');
    cell.textContent = title;
    header.appendChild(cell);
  }
  table.appendChild(header);
  const cells = run.checkpoints.map((entry, index) => {
    const row = document.createElement('tr');
    const texts = [`#${index + 1}`, stepsText(entry.steps), ...ARENAS.map(() => '–')];
    const made = texts.map((text) => {
      const cell = document.createElement('td');
      cell.textContent = text;
      row.appendChild(cell);
      return cell;
    });
    table.appendChild(row);
    return made.slice(2);
  });
  return { cells };
}

/** A column heading for a stage. */
function shortLabel(kind: ArenaKind): string {
  switch (kind) {
    case 'flat':
      return 'A';
    case 'side':
      return 'B';
    case 'obstacles':
      return 'Arena';
    case 'driving':
      return 'D';
    case 'flight':
      return 'F';
  }
}

/** Writes "landed/crashed" for each checkpoint and stage. */
function updateBoard(board: Board, panels: StagePanel[]): void {
  board.cells.forEach((row, checkpoint) => {
    row.forEach((cell, stage) => {
      const panel = panels[stage];
      if (panel && panel.rows.length > checkpoint) {
        setText(cell, `${panel.landed[checkpoint] ?? 0}/${panel.crashed[checkpoint] ?? 0}`);
      }
    });
  });
}

/** The headline, e.g. "8 checkpoints, 5 stages" (or the focused stage's name). */
function statusText(state: ViewerState): string {
  const where = state.focus === null ? `${state.panels.length} stages` : arenaLabel(state.focus);
  const reasons = new Set(
    state.panels.flatMap((panel) => (panel.error === null ? [] : [panel.error])),
  );
  const failedCount = state.panels.filter((panel) => panel.error !== null).length;
  const problem =
    reasons.size > 0
      ? ` - ${failedCount === state.panels.length ? 'all stages' : `${failedCount} stage(s)`} failed: ${[...reasons].join('; ')}`
      : '';
  return `${state.run.checkpoints.length} checkpoints, ${where}${problem}`;
}

start().catch((error: unknown) => {
  console.error(error);
  const status = document.getElementById('viewer-status');
  if (status) {
    status.textContent = String(error);
  }
});
