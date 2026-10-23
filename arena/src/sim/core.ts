// The part of the interface the sim calls, typed from the generated package.
//
// The sim imports the package for types only and receives the functions as a
// `Core`, so a test can put a fake in its place and the sim never loads
// WebAssembly. `session.ts` makes the real one.
//
// The two lists below are the one place the calls are named: the type, the
// counters, the wrappers and `makeCore` are all made from them.

import type * as Pkg from "../pkg/enzan_wasm.js";

/** The calls made on a loaded `Content`. */
export const CONTENT_CALLS = [
  "rollDrop",
  "evaluateSheet",
  "checkItem",
  "bases",
  "gainExperience",
] as const;
/** The free functions of the package that draw on a random state. */
export const RNG_CALLS = ["rngStream", "rngNextF64", "rngBelow"] as const;
/** Every call of a `Core`, as `Findings` counts them. */
export const CORE_CALLS = [...CONTENT_CALLS, ...RNG_CALLS] as const;

export type ContentCall = (typeof CONTENT_CALLS)[number];
export type RngCall = (typeof RNG_CALLS)[number];
export type CoreCall = (typeof CORE_CALLS)[number];

export type Core = Pick<Pkg.Content, ContentCall> & Pick<typeof Pkg, RngCall>;

/** Wraps one call of a `Core`, keeping its arguments and result. */
export type CallWrapper = <A extends unknown[], R>(
  name: CoreCall,
  call: (...args: A) => R,
) => (...args: A) => R;

/** A `Core` whose every call goes through `wrap`. */
export function wrapCore(core: Core, wrap: CallWrapper): Core {
  const wrapped: Partial<Record<CoreCall, unknown>> = {};
  for (const name of CORE_CALLS) {
    wrapped[name] = wrap(name, core[name] as (...args: unknown[]) => unknown);
  }
  // Every name of `Core` is in CORE_CALLS, because `Core` is made from it.
  return wrapped as Core;
}

export type {
  BaseInfo,
  ItemInput,
  ItemRoll,
  ModifierInput,
  Progress,
  RolledAffix,
  SourceInput,
} from "../pkg/enzan_wasm.js";

export type ModifierKind = Pkg.ModifierInput["modifier"];
