// What the game counts about its use of the interface. It is read by the HUD
// and the smoke, lives in `World.findings`, and is left out of a save and of the
// digest: it describes how the world was run, not what it holds.

import { CORE_CALLS, type Core, type CoreCall, wrapCore } from "./core.ts";

/** The pieces of glue, as `Findings` counts their use. */
export const GLUE_NAMES = [
  "enemyStats",
  "slotOf",
  "levelBonus",
  "playerStats",
  "hitDamage",
] as const;

export type GlueName = (typeof GLUE_NAMES)[number];

export interface Findings {
  /** Interface calls by name. */
  calls: Record<CoreCall, number>;
  /** Uses of each piece of `glue/` by name. */
  glue: Record<GlueName, number>;
  defeats: number;
  /** Throws seen going out of an interface call; the good answer is 0. */
  thrown: number;
}

function zeroed<K extends string>(names: readonly K[]): Record<K, number> {
  const counts = {} as Record<K, number>;
  for (const name of names) {
    counts[name] = 0;
  }
  return counts;
}

export function newFindings(): Findings {
  return { calls: zeroed(CORE_CALLS), glue: zeroed(GLUE_NAMES), defeats: 0, thrown: 0 };
}

/** Counts one use of a piece of glue. */
export function used(findings: Findings, name: GlueName): void {
  findings.glue[name] += 1;
}

/**
 * A `Core` that counts each call in `findings`. A throw is counted and goes on
 * out unchanged: nothing here catches it.
 */
export function meter(core: Core, findings: Findings): Core {
  return wrapCore(core, (name, call) => (...args) => {
    findings.calls[name] += 1;
    try {
      return call(...args);
    } catch (e) {
      findings.thrown += 1;
      throw e;
    }
  });
}
