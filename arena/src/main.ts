// The page: loads the package and the content, runs the sim at a fixed 60 ticks
// to the second, and does all the input and output (keys, canvas, HUD, the
// saved game in localStorage). The clock is read here and nowhere in `sim/`.

import init, {
  Content,
  contractVersion,
  isEnzanError,
  rngBelow,
  rngNextF64,
  rngStream,
  type ContentSpec,
  type ItemRoll,
} from "./pkg/enzan_wasm.js";
import contentFile from "../content.json";
import tuningFile from "../tuning.json";
import { render } from "./render.ts";
import { type Core, type ModifierKind, wrapCore } from "./sim/core.ts";
import { newFindings } from "./sim/findings.ts";
import { SLOTS, slotOf } from "./sim/glue/equip.ts";
import { loadText, saveText, stampOf } from "./sim/save.ts";
import { advance, makeCore } from "./sim/session.ts";
import type { Input } from "./sim/step.ts";
import { newWorld, type Tuning, type World } from "./sim/world.ts";

const SAVE_KEY = "enzan-arena-save";
const STEP_MS = 1000 / 60;
/** The most ticks run for one frame; a longer wait is dropped, not caught up. */
const MAX_STEPS = 5;
/** Ticks between two saves. */
const SAVE_EVERY = 300;

function byId<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`index.html has no #${id}`);
  }
  return element as T;
}

const canvas = byId<HTMLCanvasElement>("screen");
const statusBox = byId<HTMLDivElement>("status");
const playerBox = byId<HTMLPreElement>("player");
const equipmentBox = byId<HTMLPreElement>("equipment");
const findingsBox = byId<HTMLPreElement>("findings");

const tuning: Tuning = tuningFile;

function describeError(e: unknown): string {
  if (isEnzanError(e)) {
    return `${e.code}${e.at === "" ? "" : ` at ${e.at}`}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

/** A `Core` that adds up the time spent inside each call, in milliseconds. */
function timed(core: Core, clock: { ms: number }): Core {
  return wrapCore(core, (_name, call) => (...args) => {
    const start = performance.now();
    try {
      return call(...args);
    } finally {
      clock.ms += performance.now() - start;
    }
  });
}

function randomSeed(): bigint {
  return crypto.getRandomValues(new BigUint64Array(1))[0] ?? 1n;
}

/** The keys that move the player, and where each one asks to go. */
const MOVES: ReadonlyMap<string, Input> = new Map([
  ["KeyW", { dx: 0, dy: -1 }],
  ["KeyA", { dx: -1, dy: 0 }],
  ["KeyS", { dx: 0, dy: 1 }],
  ["KeyD", { dx: 1, dy: 0 }],
]);

/** The keys down now. */
const held = new Set<string>();
/**
 * The keys pressed since the last tick ran, kept until one has: a key pressed
 * and let go between two ticks still moves the player for a tick.
 */
const pressed = new Set<string>();

function currentInput(): Input {
  let dx = 0;
  let dy = 0;
  for (const code of new Set([...held, ...pressed])) {
    const move = MOVES.get(code);
    if (move !== undefined) {
      dx += move.dx;
      dy += move.dy;
    }
  }
  return { dx, dy };
}

function number(n: number): string {
  return String(Number(n.toFixed(2)));
}

/** How a modifier's value reads; a kind missing here is a compile error. */
const MODIFIER_TEXT: Record<ModifierKind, (value: number) => string> = {
  added: (value) => `+${number(value)}`,
  increased: (value) => `+${number(value * 100)}%`,
  more: (value) => `x${number(1 + value)}`,
};

function describeItem(item: ItemRoll, names: Map<number, string>): string {
  const affixes = item.affixes.map(
    (affix) => `${MODIFIER_TEXT[affix.modifier](affix.value)} ${affix.stat}`,
  );
  const name = names.get(item.base) ?? `base ${item.base}`;
  return `${name} (rarity ${item.rarity})${affixes.map((a) => `\n    ${a}`).join("")}`;
}

async function main(): Promise<void> {
  await init();
  const spec = contentFile as ContentSpec;
  const issues = Content.validate(spec);
  if (issues.length > 0) {
    statusBox.textContent = issues.map((i) => `${i.code} at ${i.at}: ${i.message}`).join("\n");
    return;
  }
  const content = Content.load(spec);
  const clock = { ms: 0 };
  const core = makeCore({ rngStream, rngNextF64, rngBelow }, content);
  const timedCore = timed(core, clock);
  const stamp = stampOf(contentFile, tuningFile, contractVersion());
  const names = new Map(spec.bases.map((base) => [base.id, base.name]));
  // Counters for drawing, so the world's own are not touched.
  const scratch = newFindings();

  const maybeContext = canvas.getContext("2d");
  if (maybeContext === null) {
    throw new Error("the browser gave no 2D canvas");
  }
  const context: CanvasRenderingContext2D = maybeContext;

  let note = "";

  function fresh(): World {
    return newWorld(randomSeed(), core, tuning);
  }

  function start(): World {
    let stored: string | null;
    try {
      stored = localStorage.getItem(SAVE_KEY);
    } catch (e) {
      note = `Could not read the saved game (${describeError(e)}). A new game started.`;
      return fresh();
    }
    if (stored === null) {
      return fresh();
    }
    const loaded = loadText(stored, stamp, core, tuning);
    if (!loaded.ok) {
      note = `The saved game was refused (${loaded.reason}). A new game started.`;
      return fresh();
    }
    note = "Resumed from the saved game.";
    return loaded.world;
  }

  let world = start();
  // The slot of a base for drawing, by the classes of the world in play.
  const slotFor = (base: number) => slotOf(world.classes, tuning, base, scratch);
  let savedAt = world.tick;
  let running = true;

  /** Writes the world. It is always whole: `advance` never leaves a tick half done. */
  function save(): void {
    try {
      localStorage.setItem(SAVE_KEY, saveText(world, stamp));
      savedAt = world.tick;
    } catch (e) {
      note = `Could not save (${describeError(e)}).`;
    }
  }

  let last: number | null = null;
  let owed = 0;
  let frameMs = 0;
  let worstMs = 0;

  function hud(): void {
    const { player, stats, equipment, findings } = world;
    statusBox.textContent = note;
    playerBox.textContent = [
      `level   ${player.progress.level}   xp ${number(player.progress.experience)}`,
      `life    ${number(player.life)} / ${number(stats.life)}`,
      `damage  ${number(stats.damage)}   crit ${number(stats.crit * 100)}%`,
      `tick    ${world.tick}   wave ${world.director.wave + 1}   enemies ${world.enemies.length}`,
      `seed    ${world.seed}`,
    ].join("\n");
    equipmentBox.textContent = SLOTS.map((slot) => {
      const item = equipment[slot];
      return `${slot}: ${item === undefined ? "-" : describeItem(item, names)}`;
    }).join("\n");
    const calls = Object.entries(findings.calls).map(([name, n]) => `${name} ${n}`);
    const glue = Object.entries(findings.glue).map(([name, n]) => `${name} ${n}`);
    findingsBox.textContent = [
      `calls   ${calls.join(", ")}`,
      `glue    ${glue.join(", ")}`,
      `defeats ${findings.defeats}   thrown ${findings.thrown}`,
      `core time  ${frameMs.toFixed(2)} ms this frame, ${worstMs.toFixed(2)} ms worst`,
    ].join("\n");
  }

  function frame(now: number): void {
    if (!running) return;
    requestAnimationFrame(frame);
    // Nothing runs while the tab is hidden, and the wait is not paid back.
    if (document.hidden) {
      last = null;
      return;
    }
    if (last !== null) owed += now - last;
    last = now;
    const due = Math.floor(owed / STEP_MS);
    const steps = Math.min(due, MAX_STEPS);
    owed = due > MAX_STEPS ? 0 : owed - steps * STEP_MS;
    clock.ms = 0;
    try {
      world = advance(world, timedCore, tuning, currentInput, steps);
      if (steps > 0) pressed.clear();
    } catch (e) {
      running = false;
      note = `Stopped: ${describeError(e)}`;
    }
    frameMs = clock.ms;
    worstMs = Math.max(worstMs, frameMs);
    if (world.tick - savedAt >= SAVE_EVERY) save();
    try {
      render(context, world, tuning, slotFor);
    } catch (e) {
      running = false;
      note = `Stopped: ${describeError(e)}`;
    }
    hud();
  }

  /** A new game, which also starts the loop again if a throw stopped it. */
  function restart(): void {
    world = fresh();
    note = "A new game started.";
    savedAt = world.tick;
    if (!running) {
      running = true;
      last = null;
      owed = 0;
      requestAnimationFrame(frame);
    }
    save();
  }

  window.addEventListener("keydown", (event) => {
    // A key with a modifier is the browser's (Ctrl+R and Cmd+R reload the page).
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.code === "KeyR") {
      if (!event.repeat) restart();
    } else if (MOVES.has(event.code)) {
      held.add(event.code);
      pressed.add(event.code);
    } else {
      return;
    }
    event.preventDefault();
  });
  window.addEventListener("keyup", (event) => held.delete(event.code));
  window.addEventListener("blur", () => {
    held.clear();
    pressed.clear();
  });
  window.addEventListener("pagehide", save);

  requestAnimationFrame(frame);
}

main().catch((e: unknown) => {
  statusBox.textContent = describeError(e);
});
