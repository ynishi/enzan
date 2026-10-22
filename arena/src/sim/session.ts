// What the page and the smoke share, so that neither builds its own core or
// its own loop of ticks.

import type * as Pkg from "../pkg/enzan_wasm.js";
import { CONTENT_CALLS, type ContentCall, type Core, type CoreCall, RNG_CALLS, type RngCall } from "./core.ts";
import { meter } from "./findings.ts";
import { type Input, step } from "./step.ts";
import type { Tuning, World } from "./world.ts";

/** The free functions of the generated package that a `Core` needs. */
export type RngFunctions = Pick<typeof Pkg, RngCall>;

/**
 * A `Core` over a loaded `Content` and the random functions of the package.
 * The methods of the content are bound to the content, so the `Core` can be
 * handed around as plain functions.
 */
export function makeCore(pkg: RngFunctions, content: Pick<Pkg.Content, ContentCall>): Core {
  const core: Partial<Record<CoreCall, unknown>> = {};
  for (const name of CONTENT_CALLS) {
    core[name] = content[name].bind(content);
  }
  for (const name of RNG_CALLS) {
    core[name] = pkg[name];
  }
  // Every name of `Core` is in one of the two lists, because `Core` is made from them.
  return core as Core;
}

/** A copy of plain data: arrays, objects, and values (bigints included). */
function copy<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map(copy) as T;
  }
  if (value !== null && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [key, entry] of Object.entries(value)) {
      out[key] = copy(entry);
    }
    return out as T;
  }
  return value;
}

/**
 * Runs `n` ticks, or fewer if the player dies, and returns the world they lead
 * to. `inputs` says what the player asks for at a tick.
 *
 * The ticks run on a copy, so `world` itself is never changed: when the
 * interface throws partway through a tick, the throw goes on out and the
 * caller still holds a whole world, never a tick half done. The calls of the
 * core are counted in `world.findings`, which the copy shares, so a call that
 * threw is counted too.
 */
export function advance(
  world: World,
  core: Core,
  tuning: Tuning,
  inputs: (tick: number) => Input,
  n: number,
): World {
  const { findings, ...rest } = world;
  const next: World = { ...copy(rest), findings };
  const counted = meter(core, findings);
  for (let i = 0; i < n && !next.over; i++) {
    step(next, inputs(next.tick), counted, tuning);
  }
  return next;
}
